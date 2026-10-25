// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::Tween;
use crate::effects::Backdrop;
use crate::scroll::ScrollState;
use crate::shape::RoundedRect;
use crate::style::{InteractionAppearance, StateStyles};
use crate::units::ScaleFactor;

fn paint(view: &mut View, scene: &mut Scene, time_ms: u64) -> TestResult {
    let time = std::time::Duration::from_millis(time_ms);
    scene.at(time_ms).show(view.frame_at(
        Viewport::new(Size::new(500.dp(), 100.dp()), Density::ONE),
        &mut Metrics,
        crate::frame::FrameTime {
            now: time,
            presentation: time,
            ..Default::default()
        },
    )?)?;
    Ok(())
}

fn frame(view: &mut View, scene: &mut Scene, time_ms: u64) -> Result<Vec<DrawCommand>, TestError> {
    paint(view, scene, time_ms)?;
    Ok(scene.commands())
}

pub(super) fn islands(commands: &[DrawCommand]) -> Result<(usize, Vec<Shape>), TestError> {
    commands
        .iter()
        .enumerate()
        .find_map(|(index, command)| match command {
            DrawCommand::Fill {
                shape: Shape::Union(union),
                ..
            } => Some((index, union.members().to_vec())),
            _ => None,
        })
        .required("a group fill over the union of its islands")
}

/// Whether the command at `index` is drawn while the clip set by the command at `clip` applies.
/// That clip applies from the command at `clip` until the `Restore` that ends the canvas state
/// saved before it.
pub(super) fn inside_clip(commands: &[DrawCommand], clip: usize, index: usize) -> bool {
    let mut depth = 1;
    clip < index
        && commands[clip..index].iter().all(|command| {
            depth += match command {
                DrawCommand::Save
                | DrawCommand::Opacity(_)
                | DrawCommand::OpacityLayer { .. }
                | DrawCommand::EffectLayer { .. } => 1,
                DrawCommand::Restore => -1,
                _ => 0,
            };
            depth > 0
        })
}

fn glass() -> Material {
    const BLUR: Backdrop = crate::constant!(Backdrop::blur(Dp::new(4.0)));
    const OFFSET: Point = Point::new(Dp::ZERO, Dp::new(1.0));
    const SHADOW: Shadow = crate::constant!(Shadow::new(Color::WHITE, Dp::new(2.0), OFFSET));
    const BORDER: Stroke = crate::constant!(Stroke::new(Color::WHITE, Dp::new(1.0)));
    Material::new(Color::WHITE)
        .backdrop(BLUR)
        .shadow(SHADOW)
        .border(BORDER)
}

pub(super) fn island(view: &View, tag: &str) -> Result<Shape, TestError> {
    Ok(RoundedRect::new(view.tagged_bounds(tag)?, Corners::all(20.dp())).into())
}

#[test]
fn material_group_paints_one_union_of_its_islands_at_their_live_bounds() -> TestResult {
    let moved = State::new(false);
    let target = moved.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let x = if target.get() { 100.dp() } else { 0.dp() };
        ui.layout(
            Axis::Horizontal,
            Modifier::new()
                .gap(10.dp())
                .padding(8.dp())
                .material_group(glass(), 24.dp()),
            |ui| {
                ui.spacer(
                    80.dp(),
                    40.dp(),
                    Modifier::new().island(Corners::all(20.dp())).tag("first"),
                );
                ui.spacer(
                    80.dp(),
                    40.dp(),
                    Modifier::new()
                        .offset(x, 0.dp())
                        .island(Corners::all(20.dp()))
                        .background(Color::WHITE)
                        .transition(Tween::new(std::time::Duration::from_secs(1)))
                        .tag("second"),
                );
                ui.spacer(20.dp(), 40.dp(), Modifier::new());
            },
        );
    });
    let commands = frame(&mut view, &mut scene, 0)?;
    let (fill, shapes) = islands(&commands)?;
    assert_eq!(shapes, [island(&view, "first")?, island(&view, "second")?]);
    let child = commands
        .iter()
        .position(|c| {
            matches!(
                c,
                DrawCommand::Fill {
                    shape: Shape::Rounded(_),
                    ..
                }
            )
        })
        .required("a rounded fill")?;
    assert!(fill < child, "the group paints below its islands");
    let Some(DrawCommand::Fill { shape, .. }) = commands.get(fill) else {
        return Err(TestError::Absent("the fill of the group at its index"));
    };
    assert!(
        matches!(commands.last(), Some(DrawCommand::Stroke { shape: border, .. }) if border == shape),
        "the group border encloses its islands"
    );

    moved.set(true);
    paint(&mut view, &mut scene, 0)?;
    let commands = frame(&mut view, &mut scene, 500)?;
    let second = view.tagged_bounds("second")?;
    assert!((second.x - 148.dp()).abs() < 0.01.dp(), "{second:?}");
    assert_eq!(islands(&commands)?.1[1], island(&view, "second")?);
    Ok(())
}

#[test]
fn a_pressed_island_shrinks_the_glass_of_its_group_with_its_content() -> TestResult {
    let pressed = StateStyles::new(|state| {
        let scale = if state.pointer.active {
            crate::constant!(ScaleFactor::try_new(0.5))
        } else {
            ScaleFactor::ONE
        };
        InteractionAppearance::default().scale(scale)
    });
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let pressed = pressed.clone();
        ui.layout(
            Axis::Horizontal,
            Modifier::new()
                .gap(10.dp())
                .material_group(glass(), 24.dp()),
            |ui| {
                ui.spacer(
                    80.dp(),
                    40.dp(),
                    Modifier::new().island(Corners::all(20.dp())).tag("first"),
                );
                ui.spacer(
                    80.dp(),
                    40.dp(),
                    Modifier::new()
                        .island(Corners::all(20.dp()))
                        .tag("second")
                        .on_click(|| {})
                        .state_styles(pressed),
                );
            },
        );
    });
    paint(&mut view, &mut scene, 0)?;
    let second = view.tagged_bounds("second")?;
    let center = Point::new(
        second.x + second.width / 2.0,
        second.y + second.height / 2.0,
    );
    view.input(InputEvent::Move(center));
    view.input(InputEvent::Press(center));
    let commands = frame(&mut view, &mut scene, 0)?;
    assert_eq!(view.bounds_of("second"), Some(second));
    let (_, members) = islands(&commands)?;
    assert_eq!(members[0], island(&view, "first")?);
    let Some(Shape::Rounded(member)) = members.get(1) else {
        return Err(TestError::Absent("a rounded shape as the second island"));
    };
    let shrunk = [second.x + 20.dp(), second.y + 10.dp(), 40.dp(), 20.dp()];
    let painted = member.rect;
    for (actual, expected) in [painted.x, painted.y, painted.width, painted.height]
        .iter()
        .zip(shrunk)
    {
        assert!((*actual - expected).abs() < 0.01.dp(), "{painted:?}");
    }
    Ok(())
}

#[test]
fn material_group_without_islands_paints_no_material() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(
            Axis::Horizontal,
            Modifier::new().material_group(glass(), 24.dp()),
            |ui| {
                ui.spacer(80.dp(), 40.dp(), Modifier::new());
            },
        );
    });
    let commands = frame(&mut view, &mut scene, 0)?;
    assert!(!commands.iter().any(|command| matches!(
        command,
        DrawCommand::Fill { .. }
            | DrawCommand::Backdrop { .. }
            | DrawCommand::Shadow { .. }
            | DrawCommand::Stroke { .. }
    )));
    Ok(())
}

#[test]
fn material_group_on_a_scrolled_node_paints_inside_the_scroll_clip() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let state = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(
            state.clone(),
            Modifier::new()
                .height(60.dp())
                .material_group(glass(), 24.dp()),
            |ui| {
                for _ in 0..3 {
                    ui.spacer(
                        80.dp(),
                        40.dp(),
                        Modifier::new().island(Corners::all(20.dp())),
                    );
                }
            },
        );
    });
    paint(&mut view, &mut scene, 0)?;
    scroll.jump_to(60.dp());
    let commands = frame(&mut view, &mut scene, 0)?;
    let clip = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Clip { rect, .. } if rect.height == 60.dp()))
        .required("a clip around the scrolled content of the node")?;
    let (fill, members) = islands(&commands)?;
    assert_eq!(members[0].bounds().y, -60.dp());
    let border = commands
        .iter()
        .rposition(|c| matches!(c, DrawCommand::Stroke { .. }))
        .required("a stroke")?;
    assert!(
        inside_clip(&commands, clip, fill),
        "the union paints inside the clip"
    );
    assert!(
        inside_clip(&commands, clip, border),
        "the border paints inside the clip"
    );
    Ok(())
}
