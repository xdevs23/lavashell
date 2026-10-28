// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{AnimationError, Tween};
use crate::scene::tree::{Change, Group, Node as SceneNode, SceneNodeId, SceneTree};
use crate::scroll::{ScrollError, ScrollState};
use std::time::Duration;

/// For each failure in the next frame of `view`, the tag of the failed component and the cause. The update of the
/// frame goes to `scene`.
fn next_failures(
    view: &mut View,
    scene: &mut Scene,
) -> Result<Vec<(Option<String>, ComponentError)>, TestError> {
    let Frame { update, failures } = framed(view, WINDOW_SIZE, 0)?;
    scene.show(update)?;
    let failures = failures.into_iter();
    Ok(failures
        .map(|failure| (failure.component.tag, failure.cause))
        .collect())
}

/// A failure of the component tagged `tag` with `cause`, as `next_failures` returns it.
fn failed(tag: &str, cause: ComponentError) -> (Option<String>, ComponentError) {
    (Some(tag.to_owned()), cause)
}

/// The components "header" and "forecast" below each other. The forecast contains the component "hourly", and fails
/// while `broken` is set.
fn weather(broken: State<bool>) -> impl FnMut(&mut Ui) + 'static {
    move |ui: &mut Ui| {
        let broken = broken.clone();
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            ui.scoped("header", Modifier::new().tag("header"), |ui| {
                ui.spacer(40.dp(), 10.dp(), Modifier::new().tag("title"));
            });
            ui.scoped("forecast", Modifier::new().tag("forecast"), move |ui| {
                ui.scoped("hourly", Modifier::new().tag("hourly"), |ui| {
                    ui.spacer(40.dp(), 10.dp(), Modifier::new().tag("hour"));
                });
                if broken.get() {
                    ui.fail(CompositionError::DuplicateItemKey);
                }
            });
        });
    }
}

#[test]
fn a_failed_component_loses_its_nodes_and_the_rest_is_reused() -> TestResult {
    let broken = State::new(false);
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), weather(broken.clone()));
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    let before = view.stats();
    broken.set(true);
    let cause = ComponentError::Composition(CompositionError::DuplicateItemKey);
    assert_eq!(
        next_failures(&mut view, &mut scene)?,
        [failed("forecast", cause)]
    );
    assert_eq!(view.bounds_of("hourly"), None);
    assert_eq!(view.bounds_of("hour"), None);
    assert!(view.bounds_of("forecast").is_some());
    assert!(view.bounds_of("title").is_some());
    let after = view.stats();
    assert_eq!(after.root_compositions, before.root_compositions);
    assert_eq!(
        after.component_compositions,
        before.component_compositions + 1
    );
    assert!(after.reused_components > before.reused_components);
    Ok(())
}

#[test]
fn a_failed_component_composes_again_only_once_a_value_it_read_changes() -> TestResult {
    let broken = State::new(true);
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), weather(broken.clone()));
    assert_eq!(next_failures(&mut view, &mut scene)?.len(), 1);
    assert!(!view.needs_frame());
    let compositions = view.stats().component_compositions;
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    assert_eq!(view.stats().component_compositions, compositions);
    broken.set(false);
    assert!(view.needs_frame());
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    assert_eq!(view.stats().component_compositions, compositions + 1);
    assert!(view.bounds_of("hour").is_some());
    Ok(())
}

#[test]
fn the_next_frame_returns_a_measured_failure_and_composes_again() -> TestResult {
    let broken = State::new(true);
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), weather(broken.clone()));
    let open = Proposal {
        width: None,
        height: None,
    };
    let _measured = view.size_that_fits(open, &mut Metrics)?;
    assert_eq!(view.bounds_of("hour"), None);
    broken.set(false);
    let cause = ComponentError::Composition(CompositionError::DuplicateItemKey);
    assert_eq!(
        next_failures(&mut view, &mut scene)?,
        [failed("forecast", cause)]
    );
    assert!(view.bounds_of("hour").is_some());
    Ok(())
}

#[test]
fn a_cycle_across_two_components_fails_the_nearest_component_around_both() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("header", Modifier::new().tag("header"), |ui| {
                ui.spacer(40.dp(), 10.dp(), Modifier::new().tag("title"));
            });
            ui.scoped("forecast", Modifier::new().tag("forecast"), |ui| {
                let (hours, days) = (NodeRef::new(), NodeRef::new());
                ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                    let (own, read) = (hours.clone(), days.clone());
                    ui.scoped("hourly", Modifier::new().tag("hourly"), move |ui| {
                        let width = Modifier::new().width(read.width());
                        ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&own));
                    });
                    let (own, read) = (days.clone(), hours.clone());
                    ui.scoped("daily", Modifier::new().tag("daily"), move |ui| {
                        let width = Modifier::new().width(read.width());
                        ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&own));
                    });
                });
            });
        });
    });
    let failures = next_failures(&mut view, &mut scene)?;
    assert!(matches!(
        failures.as_slice(),
        [(Some(tag), ComponentError::Layout(LayoutError::SizeCycle(_)))] if tag == "forecast"
    ));
    assert_eq!(view.bounds_of("hourly"), None);
    assert!(view.bounds_of("title").is_some());
    Ok(())
}

#[test]
fn a_component_emptied_by_a_cycle_inside_it_composes_again_once_the_cycle_is_gone() -> TestResult {
    let cycled = State::new(true);
    let reads = cycled.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let cycled = reads.clone();
        ui.scoped("forecast", Modifier::new().tag("forecast"), move |ui| {
            let (hours, days) = (NodeRef::new(), NodeRef::new());
            ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                let (own, read, cycled) = (hours.clone(), days.clone(), cycled.clone());
                ui.scoped("hourly", Modifier::new().tag("hourly"), move |ui| {
                    let width = if cycled.get() {
                        Modifier::new().width(read.width())
                    } else {
                        Modifier::new().width(40.dp())
                    };
                    ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&own).tag("hour"));
                });
                let (own, read) = (days.clone(), hours.clone());
                ui.scoped("daily", Modifier::new().tag("daily"), move |ui| {
                    let width = Modifier::new().width(read.width());
                    ui.spacer(Dp::ZERO, 10.dp(), width.node_ref(&own));
                });
            });
        });
    });
    let failures = next_failures(&mut view, &mut scene)?;
    assert!(matches!(
        failures.as_slice(),
        [(Some(tag), ComponentError::Layout(LayoutError::SizeCycle(_)))] if tag == "forecast"
    ));
    assert!(!view.needs_frame());
    cycled.set(false);
    assert!(view.needs_frame());
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    assert!(view.bounds_of("hour").is_some());
    Ok(())
}

#[test]
fn a_component_that_reads_a_reference_into_a_failed_component_comes_back_with_it() -> TestResult {
    let broken = State::new(true);
    let reads = broken.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (broken, row) = (reads.clone(), NodeRef::new());
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            let set = row.clone();
            ui.scoped("now", Modifier::new().tag("now"), move |ui| {
                ui.spacer(40.dp(), 10.dp(), Modifier::new().node_ref(&set));
                if broken.get() {
                    ui.fail(CompositionError::DuplicateItemKey);
                }
            });
            ui.scoped("hourly", Modifier::new().tag("hourly"), move |ui| {
                let width = Modifier::new().width(row.width());
                ui.spacer(Dp::ZERO, 10.dp(), width.tag("hour"));
            });
        });
    });
    let failures = next_failures(&mut view, &mut scene)?;
    assert!(matches!(
        failures.as_slice(),
        [
            (Some(now), ComponentError::Composition(CompositionError::DuplicateItemKey)),
            (Some(hourly), ComponentError::Layout(LayoutError::UnsetNodeRef(_))),
        ] if now == "now" && hourly == "hourly"
    ));
    assert_eq!(view.bounds_of("hour"), None);
    broken.set(false);
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    assert!(view.bounds_of("hour").is_some());
    Ok(())
}

#[test]
fn a_component_failed_by_a_layout_error_takes_no_shortcut() -> TestResult {
    let pressed = State::new(0);
    let counted = pressed.clone();
    let key = Shortcut::control(Key::Character('k'));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let counted = counted.clone();
        ui.scoped("panel", Modifier::new().tag("panel"), move |ui| {
            let counted = counted.clone();
            ui.on_shortcut(key, move || counted.update(|count| *count += 1));
            ui.spacer(20.dp(), 10.dp(), Modifier::new().weight(-1.0));
        });
    });
    let failures = next_failures(&mut view, &mut scene)?;
    assert!(matches!(
        failures.as_slice(),
        [(Some(tag), ComponentError::Layout(LayoutError::Weight { .. }))] if tag == "panel"
    ));
    view.input(InputEvent::Shortcut(key));
    assert_eq!(pressed.get(), 0);
    Ok(())
}

#[test]
fn a_component_that_each_layout_composes_again_fails_once_and_rests() -> TestResult {
    let seen = State::new(Dp::ZERO);
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (written, read) = (seen.clone(), seen.clone());
        let measured = Modifier::new()
            .tag("restless")
            .on_measure(move |size| written.set(size.width));
        ui.scoped("restless", measured, move |ui| {
            let width = if read.get() == 20.dp() {
                10.dp()
            } else {
                20.dp()
            };
            ui.spacer(width, 10.dp(), Modifier::new());
        });
    });
    assert_eq!(
        next_failures(&mut view, &mut scene)?,
        [failed("restless", ComponentError::CompositionCycle)]
    );
    assert!(!view.needs_frame());
    Ok(())
}

#[test]
fn a_scroll_state_that_cannot_scroll_fails_the_component_around_its_node() -> TestResult {
    let list = ScrollState::new(Axis::Vertical);
    list.jump_to(Dp::new(f64::NAN));
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let list = list.clone();
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            ui.scoped("header", Modifier::new().tag("header"), |ui| {
                ui.spacer(40.dp(), 10.dp(), Modifier::new().tag("title"));
            });
            ui.scoped("list", Modifier::new().tag("list"), move |ui| {
                ui.scroll(
                    list.clone(),
                    Modifier::new().size(100.dp(), 40.dp()),
                    |ui| {
                        ui.spacer(100.dp(), 80.dp(), Modifier::new().tag("row"));
                    },
                );
            });
        });
    });
    let failures = next_failures(&mut view, &mut scene)?;
    assert!(matches!(
        failures.as_slice(),
        [(Some(tag), ComponentError::Scroll(ScrollError::Jump(_)))] if tag == "list"
    ));
    assert_eq!(view.bounds_of("row"), None);
    assert!(view.bounds_of("title").is_some());
    Ok(())
}

#[test]
fn an_animation_that_cannot_paint_fails_the_component_around_its_node() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.scoped("header", Modifier::new().tag("header"), |ui| {
                ui.spacer(40.dp(), 10.dp(), Modifier::new().tag("title"));
            });
            ui.scoped("radar", Modifier::new().tag("radar"), |ui| {
                let clipped = Modifier::new()
                    .clip(Dp::new(f64::NAN))
                    .transition(Tween::new(Duration::from_secs(1)));
                ui.spacer(10.dp(), 10.dp(), clipped.tag("map"));
            });
        });
    });
    let cause = ComponentError::Paint(AnimationError::Value);
    assert_eq!(
        next_failures(&mut view, &mut scene)?,
        [failed("radar", cause)]
    );
    assert_eq!(view.bounds_of("map"), None);
    assert!(view.bounds_of("title").is_some());
    Ok(())
}

#[test]
fn the_nodes_of_a_failed_component_leave_without_their_exit() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scoped("radar", Modifier::new().tag("radar"), |ui| {
            let fading = Transition::new(Tween::new(Duration::from_secs(1))).exit(Presence::fade());
            let panel = Modifier::new().transition(fading).tag("panel");
            ui.layout(Axis::Vertical, panel, |ui| {
                let clipped = Modifier::new()
                    .clip(Dp::new(f64::NAN))
                    .transition(Tween::new(Duration::from_secs(1)));
                ui.spacer(10.dp(), 10.dp(), clipped);
            });
        });
    });
    let cause = ComponentError::Paint(AnimationError::Value);
    assert_eq!(
        next_failures(&mut view, &mut scene)?,
        [failed("radar", cause)]
    );
    assert_eq!(view.bounds_of("panel"), None);
    Ok(())
}

#[test]
fn a_failed_root_composition_empties_the_view_and_the_frame_still_returns() -> TestResult {
    let broken = State::new(false);
    let reads = broken.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let square = Modifier::new().background(Color::WHITE).tag("square");
        ui.spacer(10.dp(), 10.dp(), square);
        if reads.get() {
            ui.fail(NumberError::OutOfRange);
        }
    });
    assert!(next_failures(&mut view, &mut scene)?.is_empty());
    assert!(!scene.commands().is_empty());
    broken.set(true);
    let cause = ComponentError::Composition(CompositionError::Number(NumberError::OutOfRange));
    assert_eq!(next_failures(&mut view, &mut scene)?, [(None, cause)]);
    assert!(scene.commands().is_empty());
    assert_eq!(view.bounds_of("square"), None);
    Ok(())
}

#[test]
fn the_next_frame_returns_the_failures_of_a_frame_whose_update_fails() -> TestResult {
    let (count, broken) = (State::new(1), State::new(false));
    let (rows, reads) = (count.clone(), broken.clone());
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        for row in 0..rows.get() {
            let broken = reads.clone();
            ui.scoped(row, Modifier::new(), move |ui| {
                if row == 0 && broken.get() {
                    ui.fail(CompositionError::DuplicateItemKey);
                }
                ui.spacer(20.dp(), 20.dp(), Modifier::new().background(Color::WHITE));
            });
        }
    });
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    let mut tree = SceneTree::default();
    (view.show_frame(&mut tree, viewport, &mut Metrics)).without_failures()?;
    // The ids from 3 on are free after the first frame. Taking them makes the update that adds a row fail.
    let planted = (3..64).map(|id| Change::Insert {
        id: SceneNodeId(id),
        parent: None,
        index: 1,
        node: SceneNode::Group(Group::default()),
    });
    tree.show(planted.collect())?;
    count.set(2);
    broken.set(true);
    assert!(matches!(
        view.show_frame(&mut tree, viewport, &mut Metrics),
        Err(FrameError::Apply(_))
    ));
    let failures = view.show_frame(&mut tree, viewport, &mut Metrics)?;
    let causes: Vec<_> = (failures.into_iter())
        .map(|failure| failure.cause)
        .collect();
    assert_eq!(
        causes,
        [ComponentError::Composition(
            CompositionError::DuplicateItemKey
        )]
    );
    Ok(())
}
