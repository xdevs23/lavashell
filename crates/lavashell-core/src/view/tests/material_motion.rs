// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Easing, Tween};
use crate::color::HuePath;
use crate::color::samples::{BLUE, VIOLET, assert_halfway_from_blue_to_violet};
use crate::preferences::REDUCED_MOTION;
use crate::shape::RoundedRect;
use crate::style::{InteractionAppearance, StateStyles};
use std::{cell::Cell, rc::Rc, time::Duration};

pub(super) fn linear() -> Tween {
    Tween::new(Duration::from_secs(1)).easing(Easing::Linear)
}

/// A rounded rectangle filled with one solid color, found in a list of draw commands. `rect` is the
/// rectangle in screen coordinates. `scale` is the horizontal and vertical scale of the transform
/// that applied to the fill. `corners` are the corner radii before that scale.
#[derive(Debug)]
pub(super) struct PaintedFill {
    pub rect: Rect,
    pub scale: (f64, f64),
    pub corners: Corners,
    pub color: Color,
}

pub(super) fn painted_fills(commands: &[DrawCommand]) -> Result<Vec<PaintedFill>, TestError> {
    let mut stack = vec![Affine::scale(1.0, 1.0)];
    let mut fills = Vec::new();
    for command in commands {
        let current = *stack.last().required("a transform on the stack")?;
        match command {
            DrawCommand::Save
            | DrawCommand::OpacityLayer { .. }
            | DrawCommand::Opacity(_)
            | DrawCommand::EffectLayer { .. } => stack.push(current),
            DrawCommand::Restore => {
                let _popped_from_stack = stack.pop();
            }
            DrawCommand::Transform(inner) => {
                let [sx, _, _, sy] = current.linear;
                let [ix, _, _, iy] = inner.linear;
                let top = stack.last_mut().required("a transform on the stack")?;
                *top = Affine::scale(sx * ix, sy * iy).translated(
                    current
                        .translation
                        .offset(inner.translation.x * sx, inner.translation.y * sy),
                );
            }
            DrawCommand::Fill {
                shape: Shape::Rounded(RoundedRect { rect, corners }),
                brush: Brush::Solid(color),
            } => {
                let [sx, _, _, sy] = current.linear;
                let at = current.translation;
                fills.push(PaintedFill {
                    rect: Rect::new(
                        at.x + rect.x * sx,
                        at.y + rect.y * sy,
                        rect.width * sx,
                        rect.height * sy,
                    ),
                    scale: (sx, sy),
                    corners: *corners,
                    color: *color,
                });
            }
            _ => {}
        }
    }
    Ok(fills)
}

pub(super) fn close(a: Dp, b: Dp) {
    assert!((a - b).abs() < 0.01.dp(), "{a:?} != {b:?}");
}

fn resizing_view(wide: &State<bool>) -> View {
    let wide = wide.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let (width, color, corners, border) = if wide.get() {
            let border = crate::constant!(Stroke::new(VIOLET, Dp::new(4.0)));
            (300.dp(), VIOLET, 20.dp(), border)
        } else {
            let border = crate::constant!(Stroke::new(BLUE, Dp::new(2.0)));
            (100.dp(), BLUE, 10.dp(), border)
        };
        ui.spacer(
            width,
            40.dp(),
            Modifier::new()
                .material(Material::new(color).border(border), corners)
                .transition(linear()),
        );
    })
}

#[test]
fn a_bounds_transition_paints_the_material_unscaled_at_the_animated_bounds() -> TestResult {
    let wide = State::new(false);
    let mut scene = Scene::default();
    let mut view = resizing_view(&wide);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    wide.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let [fill] = painted_fills(&scene.commands())?.exactly::<1>("material fills")?;
    assert_eq!(fill.scale, (1.0, 1.0));
    close(fill.rect.width, 200.dp());
    close(fill.rect.height, 40.dp());
    close(fill.corners.top_left, 15.dp());
    assert_halfway_from_blue_to_violet(fill.color, HuePath::Shorter);
    let border = scene.commands().iter().find_map(|command| match command {
        DrawCommand::Stroke { stroke, .. } => Some(stroke.width.get()),
        _ => None,
    });
    close(border.required("a stroke width")?, 3.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let [fill] = painted_fills(&scene.commands())?.exactly::<1>("material fills")?;
    close(fill.rect.width, 300.dp());
    close(fill.corners.top_left, 20.dp());
    Ok(())
}

#[test]
fn reduced_motion_changes_the_material_of_a_transition_at_once() -> TestResult {
    let wide = State::new(false);
    let mut scene = Scene::default();
    let mut view = resizing_view(&wide);
    view.provide(&REDUCED_MOTION, true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    wide.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let [fill] = painted_fills(&scene.commands())?.exactly::<1>("material fills")?;
    close(fill.rect.width, 300.dp());
    close(fill.corners.top_left, 20.dp());
    assert_eq!(fill.color.to_argb8(), VIOLET.to_argb8());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_material_that_a_transitioned_node_loses_fades_out() -> TestResult {
    let shown = State::new(true);
    let visible = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let modifier = Modifier::new();
        let modifier = if visible.get() {
            modifier.material(Material::new(BLUE), 10.dp())
        } else {
            modifier
        };
        ui.spacer(100.dp(), 40.dp(), modifier.transition(linear()));
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let [fill] = painted_fills(&scene.commands())?.exactly::<1>("fading material fills")?;
    assert!((fill.color.alpha() - 0.5).abs() < 1e-3, "{fill:?}");
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert!(painted_fills(&scene.commands())?.is_empty());
    Ok(())
}

#[test]
fn a_lost_material_fades_out_where_it_was_in_the_modifier_chain() -> TestResult {
    let shown = State::new(true);
    let visible = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let modifier = Modifier::new().padding(10.dp()).opacity(0.5);
        let modifier = if visible.get() {
            modifier.material(Material::new(BLUE), 10.dp())
        } else {
            modifier
        };
        ui.spacer(100.dp(), 40.dp(), modifier.transition(linear()));
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let [shown_fill] = painted_fills(&scene.commands())?.exactly::<1>("material fills")?;
    let shown_rect = shown_fill.rect;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let [fill] = painted_fills(&scene.commands())?.exactly::<1>("fading material fills")?;
    assert_eq!(fill.rect, shown_rect);
    assert!((fill.color.alpha() - 0.5).abs() < 1e-3, "{fill:?}");
    let opacity = scene
        .commands()
        .iter()
        .position(|c| matches!(c, DrawCommand::Opacity(_)));
    let material = scene
        .commands()
        .iter()
        .position(|c| matches!(c, DrawCommand::Fill { .. }));
    assert!(
        opacity < material,
        "the fading material paints inside the opacity of the node"
    );
    Ok(())
}

#[test]
fn a_bounds_only_transition_changes_the_material_at_once() -> TestResult {
    let blue = State::new(true);
    let is_blue = blue.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let color = if is_blue.get() { BLUE } else { VIOLET };
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .material(Material::new(color), 10.dp())
                .transition(Transition::new(linear()).bounds_only()),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    blue.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(painted_fills(&scene.commands())?[0].color, VIOLET);
    assert!(!view.needs_frame());
    Ok(())
}

fn glass_styles() -> StateStyles {
    StateStyles::new(|state| {
        let (color, corners) = match (state.pointer.active, state.selected) {
            (true, _) => (VIOLET, 20.dp()),
            (false, true) => (Color::WHITE, 30.dp()),
            (false, false) => (BLUE, 10.dp()),
        };
        InteractionAppearance::default().material(Material::new(color), corners)
    })
    .motion(linear())
}

const CENTER: Point = Point::new(Dp::new(50.0), Dp::new(20.0));

fn styled_view(selected: &State<bool>) -> Result<(View, Scene, Rc<Cell<u32>>), TestError> {
    let (selected, compositions) = (selected.clone(), Rc::new(Cell::new(0)));
    let count = compositions.clone();
    let styles = glass_styles();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        count.set(count.get() + 1);
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new()
                .on_click(|| {})
                .selected(selected.get())
                .state_styles(styles.clone())
                .background(Color::rgb(0x0000_ff00)),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, scene, compositions))
}

#[test]
fn a_state_style_moves_the_material_of_each_state_without_composing() -> TestResult {
    let (mut view, mut scene, compositions) = styled_view(&State::new(false))?;
    let fills = painted_fills(&scene.commands())?;
    assert_eq!(fills[0].color, BLUE);
    close(fills[0].corners.top_left, 10.dp());
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_halfway_from_blue_to_violet(fill.color, HuePath::Shorter);
    close(fill.corners.top_left, 15.dp());
    assert_eq!(compositions.get(), 1);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_eq!(fill.color.to_argb8(), VIOLET.to_argb8());
    close(fill.corners.top_left, 20.dp());
    Ok(())
}

#[test]
fn a_press_released_halfway_moves_back_from_the_halfway_material() -> TestResult {
    let (mut view, mut scene, _) = styled_view(&State::new(false))?;
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    view.input(InputEvent::Release(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_halfway_from_blue_to_violet(fill.color, HuePath::Shorter);
    close(fill.corners.top_left, 15.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_eq!(fill.color.to_argb8(), BLUE.to_argb8());
    close(fill.corners.top_left, 10.dp());
    Ok(())
}

#[test]
fn a_state_style_resolves_a_state_once_while_its_appearance_moves() -> TestResult {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let styles = StateStyles::new(move |state| {
        counted.set(counted.get() + 1);
        let color = if state.pointer.active { VIOLET } else { BLUE };
        InteractionAppearance::default().material(Material::new(color), 10.dp())
    })
    .motion(linear());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.spacer(
            100.dp(),
            40.dp(),
            Modifier::new().on_click(|| {}).state_styles(styles.clone()),
        );
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let at_rest = calls.get();
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    for milliseconds in [0, 250, 500, 750, 1000] {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, milliseconds)?;
    }
    assert_eq!(calls.get(), at_rest + 1);
    Ok(())
}

#[test]
fn a_state_material_paints_below_the_decorations_of_its_node() -> TestResult {
    let (_, scene, _) = styled_view(&State::new(false))?;
    let colors: Vec<_> = painted_fills(&scene.commands())?
        .iter()
        .map(|fill| fill.color)
        .collect();
    assert_eq!(colors, [BLUE, Color::rgb(0x0000_ff00)]);
    Ok(())
}

#[test]
fn a_selected_node_moves_to_the_material_of_its_selected_state() -> TestResult {
    let selected = State::new(false);
    let (mut view, mut scene, _) = styled_view(&selected)?;
    selected.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    close(
        painted_fills(&scene.commands())?[0].corners.top_left,
        20.dp(),
    );
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_eq!(fill.color.to_argb8(), Color::WHITE.to_argb8());
    close(fill.corners.top_left, 30.dp());
    Ok(())
}

#[test]
fn reduced_motion_changes_the_material_of_a_state_at_once() -> TestResult {
    let (mut view, mut scene, _) = styled_view(&State::new(false))?;
    view.provide(&REDUCED_MOTION, true);
    view.input(InputEvent::Move(CENTER));
    view.input(InputEvent::Press(CENTER));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let fill = &painted_fills(&scene.commands())?[0];
    assert_eq!(fill.color.to_argb8(), VIOLET.to_argb8());
    close(fill.corners.top_left, 20.dp());
    assert!(!view.needs_frame());
    Ok(())
}
