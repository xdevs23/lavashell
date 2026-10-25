// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{Easing, Tween};
use crate::preferences::REDUCED_MOTION;
use crate::style::{InteractionAppearance, StateStyles};
use crate::units::Fraction;
use std::time::Duration;

const PIECE: Color = Color::rgb(0x0040_a0ff);
const WELL: Color = Color::rgb(0x0030_5080);

#[path = "exits/departures.rs"]
mod departures;
#[path = "exits/returns.rs"]
mod returns;

fn linear(ms: u64) -> Tween {
    Tween::new(Duration::from_millis(ms))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

/// A transition that fades a removed node out over `ms` milliseconds and moves the bounds of the
/// node to each new layout at once.
fn fading_out(ms: u64) -> Transition {
    Transition {
        bounds: BoundsMotion::Instant,
        ..Transition::new(linear(ms))
    }
    .exit(Presence::fade())
}

fn fills(scene: &Scene, color: Color) -> Result<Vec<Rect>, TestError> {
    Ok((painted_fills(&scene.commands())?.into_iter())
        .filter(|fill| fill.color.to_argb8() == color.to_argb8())
        .map(|fill| fill.rect)
        .collect())
}

fn presences(scene: &Scene) -> Vec<f64> {
    (scene.groups().into_iter())
        .map(|group| group.opacity)
        .collect()
}

fn assert_near(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} != {expected:?}");
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 1e-4, "{actual:?} != {expected:?}");
    }
}

fn well(ui: &mut Ui, width: crate::animation::Animation<Dp>) {
    ui.canvas(Size::new(200.dp(), 20.dp()), Modifier::new(), move |d| {
        d.fill(Rect::new(0.dp(), 0.dp(), width.get(), 20.dp()), WELL);
    });
}

/// A view with a node keyed "parent" that is shown while `parent` is true and fades out over
/// `parent_ms` milliseconds when removed. Inside it is a node keyed "child", filled with `PIECE`,
/// that is shown while `child` is true and fades out over `child_ms` milliseconds when removed.
fn parent_and_child(
    parent: &State<bool>,
    child: &State<bool>,
    parent_ms: u64,
    child_ms: u64,
) -> View {
    let (parent, child) = (parent.clone(), child.clone());
    View::new(crate::platform::fixture::facts(), move |ui| {
        if parent.get() {
            let child = child.get();
            ui.key(
                "parent",
                Modifier::new().transition(fading_out(parent_ms)),
                |ui| {
                    if child {
                        ui.key(
                            "child",
                            Modifier::new()
                                .background(PIECE)
                                .transition(fading_out(child_ms)),
                            |ui| {
                                ui.spacer(20.dp(), 20.dp(), Modifier::new());
                            },
                        );
                    }
                },
            );
        }
    })
}

/// A view with a node keyed "well" that is shown while `shown` is true and fades out over a second
/// when removed. The node contains a bar in the color `WELL`, whose width grows from 40 dp to 200
/// dp over a second as the node enters, and shrinks back to 40 dp as the node leaves.
fn presence_well(shown: &State<bool>) -> View {
    let present = shown.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key(
                "well",
                Modifier::new().transition(fading_out(1000)),
                |ui| match ui.animate_presence("width", 200.dp(), 40.dp(), linear(1000)) {
                    Ok(width) => well(ui, width),
                    Err(error) => ui.fail(error),
                },
            );
        }
    })
}

#[test]
fn a_node_removed_in_the_middle_of_a_move_finishes_the_move_while_it_fades() -> TestResult {
    let (moved, shown) = (State::new(false), State::new(true));
    let (target, present) = (moved.clone(), shown.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let x = if target.get() { 200.dp() } else { 0.dp() };
            ui.key("slot", Modifier::new().transition(fading_out(1000)), |ui| {
                ui.key(
                    "piece",
                    Modifier::new()
                        .offset(x, 0.dp())
                        .background(PIECE)
                        .transition(Transition::new(linear(1000)).reflow()),
                    |ui| {
                        ui.spacer(20.dp(), 20.dp(), Modifier::new());
                    },
                );
            });
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    moved.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let piece = |x: Dp| vec![Rect::new(x, 0.dp(), 20.dp(), 20.dp())];
    assert_eq!(fills(&scene, PIECE)?, piece(100.dp()));
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_eq!(fills(&scene, PIECE)?, piece(150.dp()));
    assert_near(&presences(&scene), &[0.75, 1.0]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(fills(&scene, PIECE)?, piece(200.dp()));
    assert_near(&presences(&scene), &[0.5, 1.0]);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert!(fills(&scene, PIECE)?.is_empty());
    assert_eq!(view.transitions.count(), 0);
    Ok(())
}

#[test]
fn content_drawn_from_an_animation_goes_on_animating_while_its_node_leaves() -> TestResult {
    let (wide, shown) = (State::new(false), State::new(true));
    let (target, present) = (wide.clone(), shown.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let width = if target.get() { 200.dp() } else { 100.dp() };
            ui.key(
                "well",
                Modifier::new().transition(fading_out(1000)),
                |ui| match ui.animate("width", width, linear(1000)) {
                    Ok(width) => well(ui, width),
                    Err(error) => ui.fail(error),
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    wide.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    let rect = Rect::new(0.dp(), 0.dp(), 175.dp(), 20.dp());
    assert_eq!(fills(&scene, WELL)?, [rect]);
    assert_near(&presences(&scene), &[0.75]);
    Ok(())
}

#[test]
fn a_presence_animation_grows_from_the_exit_value_and_shrinks_back_to_it_while_the_node_around_it_leaves()
-> TestResult {
    let shown = State::new(true);
    let mut scene = Scene::default();
    let mut view = presence_well(&shown);
    let width = |scene: &Scene| fills(scene, WELL).map(|rects| rects[0].width);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(width(&scene)?, 40.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(width(&scene)?, 200.dp());
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(width(&scene)?, 120.dp());
    assert_near(&presences(&scene), &[0.5]);
    Ok(())
}

#[test]
fn a_presence_animation_at_the_root_of_a_recomposed_scoped_component_moves_to_its_exit_value()
-> TestResult {
    let (shown, height) = (State::new(true), State::new(20.dp()));
    let (present, tall) = (shown.clone(), height.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let tall = tall.clone();
            ui.scoped(
                "well",
                Modifier::new().transition(fading_out(1000)),
                move |ui| {
                    let height = tall.get();
                    let animated = ui.animate_presence("width", 200.dp(), 40.dp(), linear(1000));
                    let width = match animated {
                        Ok(width) => width,
                        Err(error) => {
                            ui.fail(error);
                            return;
                        }
                    };
                    ui.canvas(Size::new(200.dp(), height), Modifier::new(), move |d| {
                        d.fill(Rect::new(0.dp(), 0.dp(), width.get(), height), WELL);
                    });
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    height.set(30.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(view.stats().component_compositions, 1);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(
        fills(&scene, WELL)?,
        [Rect::new(0.dp(), 0.dp(), 120.dp(), 30.dp())]
    );
    Ok(())
}

#[test]
fn a_leaving_node_is_skipped_by_input_focus_and_accessibility_while_it_paints() -> TestResult {
    let (shown, clicks) = (State::new(true), State::new(0));
    let (present, count) = (shown.clone(), clicks.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            let count = count.clone();
            ui.key(
                "button",
                Modifier::new()
                    .background(PIECE)
                    .on_click(move || count.update(|taps| *taps += 1))
                    .accessible_name("leaving")
                    .transition(fading_out(1000)),
                |ui| {
                    ui.spacer(40.dp(), 40.dp(), Modifier::new());
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(TAB);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let button = view.focus().required("a focused node after the tab key")?;
    assert!(view.accessibility().nodes.contains_key(&button));
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    assert_ne!(view.focus(), Some(button));
    assert!(!view.accessibility().nodes.contains_key(&button));
    tap_at(&mut view, Point::new(20.dp(), 20.dp()));
    assert_eq!(clicks.get(), 0);
    Ok(())
}

#[test]
fn the_children_of_a_leaving_node_move_on_to_their_targets_without_starting_exits() -> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key(
                "parent",
                Modifier::new().transition(fading_out(500)),
                |ui| {
                    ui.key(
                        "child",
                        Modifier::new().transition(
                            Transition::new(linear(1000))
                                .enter(Presence::fade())
                                .exit(Presence::fade()),
                        ),
                        |ui| {
                            ui.spacer(20.dp(), 20.dp(), Modifier::new());
                        },
                    );
                },
            );
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert_near(&presences(&scene), &[1.0, 0.5]);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_near(&presences(&scene), &[0.5, 0.75]);
    Ok(())
}

fn half_opaque_on_hover(ms: u64) -> StateStyles {
    StateStyles::new(|state| {
        InteractionAppearance::default().opacity(if state.pointer.hovered {
            crate::constant!(Fraction::try_new(0.5))
        } else {
            crate::constant!(Fraction::try_new(1.0))
        })
    })
    .motion(linear(ms))
}

fn opacities(scene: &Scene) -> Vec<f64> {
    (scene.leaf_commands().into_iter())
        .filter_map(|command| match command {
            DrawCommand::Opacity(alpha) => Some(alpha),
            _ => None,
        })
        .collect()
}

#[test]
fn siblings_take_the_space_of_a_leaving_node_at_once() -> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let present = present.get();
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            if present {
                ui.key(
                    "leaving",
                    Modifier::new()
                        .background(PIECE)
                        .transition(fading_out(1000)),
                    |ui| {
                        ui.spacer(40.dp(), 20.dp(), Modifier::new());
                    },
                );
            }
            ui.spacer(20.dp(), 20.dp(), Modifier::new().tag("sibling"));
        });
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("sibling")?.x, 40.dp());
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(view.tagged_bounds("sibling")?.x, 0.dp());
    let leaving = Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp());
    assert_eq!(fills(&scene, PIECE)?, [leaving]);
    Ok(())
}

#[test]
fn reduced_motion_ends_an_exit_at_once() -> TestResult {
    let shown = State::new(true);
    let present = shown.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if present.get() {
            ui.key(
                "piece",
                Modifier::new()
                    .background(PIECE)
                    .transition(fading_out(1000)),
                |ui| {
                    ui.spacer(20.dp(), 20.dp(), Modifier::new());
                },
            );
        }
    });
    view.provide(&REDUCED_MOTION, true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert!(fills(&scene, PIECE)?.is_empty());
    assert_eq!(view.transitions.count(), 0);
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_node_leaving_inside_a_parent_that_starts_to_leave_goes_on_with_its_exit() -> TestResult {
    let (child_shown, parent_shown) = (State::new(true), State::new(true));
    let mut scene = Scene::default();
    let mut view = parent_and_child(&parent_shown, &child_shown, 1000, 1000);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    child_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    parent_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 750)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    assert_near(&presences(&scene), &[0.75, 0.25]);
    Ok(())
}

#[test]
fn a_node_leaving_inside_a_parent_whose_exit_ends_stops_painting_with_it() -> TestResult {
    let (child_shown, parent_shown) = (State::new(true), State::new(true));
    let mut scene = Scene::default();
    let mut view = parent_and_child(&parent_shown, &child_shown, 500, 2000);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    child_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    parent_shown.set(false);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 250)?;
    assert_eq!(fills(&scene, PIECE)?.len(), 1);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!(fills(&scene, PIECE)?.is_empty());
    assert_eq!(view.transitions.count(), 0);
    assert!(!view.needs_frame());
    Ok(())
}

fn card(ui: &mut Ui, x: Dp) {
    ui.shared(
        "card",
        Modifier::new()
            .offset(x, 0.dp())
            .background(PIECE)
            .transition(Transition::new(linear(1000))),
        |ui| {
            ui.spacer(20.dp(), 20.dp(), Modifier::new());
        },
    );
}

#[test]
fn a_shared_node_composed_elsewhere_paints_there_and_not_in_a_leaving_subtree() -> TestResult {
    let detail = State::new(false);
    let shows = detail.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if shows.get() {
            ui.key("detail", Modifier::new(), |ui| card(ui, 200.dp()));
        } else {
            ui.key("list", Modifier::new().transition(fading_out(1000)), |ui| {
                card(ui, 0.dp());
            });
        }
    });
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    detail.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    let moving = Rect::new(100.dp(), 0.dp(), 20.dp(), 20.dp());
    assert_eq!(fills(&scene, PIECE)?, [moving]);
    assert_near(&presences(&scene), &[0.5, 1.0]);
    Ok(())
}
