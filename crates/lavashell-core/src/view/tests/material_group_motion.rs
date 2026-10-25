// SPDX-License-Identifier: GPL-3.0-only
use super::material_group::{island, islands};
use super::material_motion::linear;
use super::*;
use crate::color::samples::{BLUE, VIOLET, assert_halfway_from_blue_to_violet};
use crate::color::{ColorInterpolation, HuePath};
use crate::float_asserts::{assert_f64_eq, assert_f64s_eq};
use crate::preferences::REDUCED_MOTION;

/// A view with a horizontal row of two islands tagged "first" and "second", painted once into the
/// returned scene. `group` maps the value of `state` to the gap between the islands and to the
/// modifier of the row. `transition`, when given, animates the row between those modifiers.
fn group_view(
    state: &State<bool>,
    transition: Option<Transition>,
    group: fn(bool) -> (Dp, Modifier),
) -> Result<(View, Scene), TestError> {
    let state = state.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (gap, modifier) = group(state.get());
        let modifier = Modifier::new().gap(gap).then(modifier);
        let modifier = match transition {
            Some(transition) => modifier.transition(transition),
            None => modifier,
        };
        ui.layout(Axis::Horizontal, modifier, |ui| {
            for tag in ["first", "second"] {
                ui.spacer(
                    80.dp(),
                    40.dp(),
                    Modifier::new().island(Corners::all(20.dp())).tag(tag),
                );
            }
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, scene))
}

fn tinted(violet: bool) -> (Dp, Modifier) {
    let tint = if violet { VIOLET } else { BLUE };
    let modifier = Modifier::new().material_group(Material::new(tint), 24.dp());
    (10.dp(), modifier)
}

fn grouped(grouped: bool) -> (Dp, Modifier) {
    let modifier = Modifier::new();
    let modifier = if grouped {
        modifier.material_group(Material::new(BLUE), 24.dp())
    } else {
        modifier
    };
    (10.dp(), modifier)
}

fn group_fills(commands: &[DrawCommand]) -> Vec<(Dp, Color)> {
    (commands.iter())
        .filter_map(|command| match command {
            DrawCommand::Fill {
                shape: Shape::Union(union),
                brush: Brush::Solid(color),
            } => Some((union.spacing(), *color)),
            _ => None,
        })
        .collect()
}

fn group_fill(scene: &Scene) -> Result<(Dp, Color), TestError> {
    let [fill] = group_fills(&scene.commands()).exactly::<1>("group fills")?;
    Ok(fill)
}

fn group_color(scene: &Scene) -> Result<Color, TestError> {
    Ok(group_fill(scene)?.1)
}

#[test]
fn a_changed_group_material_moves_in_oklch_and_settles_at_the_target() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, Some(Transition::new(linear())), tinted)?;
    assert_eq!(group_color(&scene)?, BLUE);
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(group_color(&scene)?.to_argb8(), BLUE.to_argb8());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_halfway_from_blue_to_violet(group_color(&scene)?, HuePath::Shorter);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(group_color(&scene)?.to_argb8(), VIOLET.to_argb8());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_group_material_moves_in_the_color_space_of_the_transition() -> TestResult {
    let violet = State::new(false);
    let transition =
        Transition::new(linear()).color_interpolation(ColorInterpolation::Oklch(HuePath::Longer));
    let (mut view, mut scene) = group_view(&violet, Some(transition), tinted)?;
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_halfway_from_blue_to_violet(group_color(&scene)?, HuePath::Longer);
    Ok(())
}

#[test]
fn the_islands_fuse_on_every_frame_of_a_group_material_motion() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, Some(Transition::new(linear())), tinted)?;
    violet.set(true);
    for milliseconds in (0..=1000).step_by(100) {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, milliseconds)?;
        let (_, members) = islands(&scene.commands())?;
        assert_eq!(members, [island(&view, "first")?, island(&view, "second")?]);
        assert_eq!(group_fill(&scene)?.0, 24.dp());
    }
    Ok(())
}

#[test]
fn a_group_material_moves_with_islands_that_move_in_a_reflow() -> TestResult {
    let moved = State::new(false);
    let state = moved.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let moved = state.get();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let lead = if moved { 120.dp() } else { 20.dp() };
            ui.spacer(lead, 40.dp(), Modifier::new());
            let tint = if moved { VIOLET } else { BLUE };
            ui.layout(
                Axis::Horizontal,
                Modifier::new()
                    .gap(10.dp())
                    .material_group(Material::new(tint), 24.dp())
                    .transition(Transition::new(linear()).reflow()),
                |ui| {
                    for tag in ["first", "second"] {
                        ui.spacer(
                            80.dp(),
                            40.dp(),
                            Modifier::new().island(Corners::all(20.dp())).tag(tag),
                        );
                    }
                },
            );
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = view.tagged_bounds("first")?.x;
    moved.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let (index, members) = islands(&scene.commands())?;
    let shift = (scene.commands()[..index].iter().rev())
        .find_map(|command| match command {
            DrawCommand::Transform(affine) => Some(*affine),
            _ => None,
        })
        .required("a transform on the content of the reflowing node")?;
    assert_f64s_eq(&shift.linear, &Affine::IDENTITY.linear);
    let painted: Vec<_> = (members.iter())
        .map(|member| {
            let bounds = member.bounds();
            let (x, y) = (shift.translation.x, shift.translation.y);
            Rect::new(bounds.x + x, bounds.y + y, bounds.width, bounds.height)
        })
        .collect();
    let first = view.tagged_bounds("first")?;
    assert!(start < first.x && first.x < 120.dp(), "{first:?}");
    assert_eq!(painted, [first, view.tagged_bounds("second")?]);
    assert_halfway_from_blue_to_violet(group_color(&scene)?, HuePath::Shorter);
    Ok(())
}

#[test]
fn a_group_without_a_transition_changes_its_material_at_once() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, None, tinted)?;
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(group_color(&scene)?, VIOLET);
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn reduced_motion_changes_a_group_material_at_once() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, Some(Transition::new(linear())), tinted)?;
    view.provide(&REDUCED_MOTION, true);
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(group_color(&scene)?.to_argb8(), VIOLET.to_argb8());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_group_material_changed_back_halfway_turns_back_from_the_halfway_color() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, Some(Transition::new(linear())), tinted)?;
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    violet.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_halfway_from_blue_to_violet(group_color(&scene)?, HuePath::Shorter);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(group_color(&scene)?.to_argb8(), BLUE.to_argb8());
    Ok(())
}

#[test]
fn a_group_material_moves_without_composing() -> TestResult {
    let violet = State::new(false);
    let (mut view, mut scene) = group_view(&violet, Some(Transition::new(linear())), tinted)?;
    violet.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let composed = view.stats().root_compositions;
    for milliseconds in [250, 500, 750, 1000] {
        show_frame(&mut scene, &mut view, WINDOW_SIZE, milliseconds)?;
    }
    assert_eq!(view.stats().root_compositions, composed);
    Ok(())
}

#[test]
fn a_gained_group_fades_in_from_a_transparent_material() -> TestResult {
    let shown = State::new(false);
    let (mut view, mut scene) = group_view(&shown, Some(Transition::new(linear())), grouped)?;
    assert!(group_fills(&scene.commands()).is_empty());
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_f64_eq(group_color(&scene)?.alpha(), 0.0);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(group_color(&scene)?.to_argb8(), BLUE.to_argb8());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_lost_group_fades_out_over_the_islands_with_the_last_spacing() -> TestResult {
    let shown = State::new(true);
    let (mut view, mut scene) = group_view(&shown, Some(Transition::new(linear())), grouped)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let (spacing, color) = group_fill(&scene)?;
    assert_eq!(spacing, 24.dp());
    assert!((color.alpha() - 0.5).abs() < 1e-3, "{color:?}");
    let (_, members) = islands(&scene.commands())?;
    assert_eq!(members, [island(&view, "first")?, island(&view, "second")?]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert!(group_fills(&scene.commands()).is_empty());
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_group_regained_halfway_fades_in_from_the_halfway_color() -> TestResult {
    let shown = State::new(true);
    let (mut view, mut scene) = group_view(&shown, Some(Transition::new(linear())), grouped)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let halfway = group_color(&scene)?;
    shown.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(group_color(&scene)?, halfway);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(group_color(&scene)?.to_argb8(), BLUE.to_argb8());
    Ok(())
}
