// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scene::tree::{Change, Slot, SlotChange};
use crate::scroll::ScrollState;
use std::time::Duration;

/// A view with a list 100 dp high that scrolls `scroll` on a spring. The list contains one row 40
/// dp high for each counter in `taps`. Each row counts its taps in its counter. The list and every
/// row are scoped components.
fn scoped_list(scroll: &ScrollState, taps: &[State<u32>]) -> View {
    let list = scroll.clone();
    let taps = taps.to_vec();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let (list, taps) = (list.clone(), taps.clone());
        ui.scoped("list", Modifier::new(), move |ui| {
            let spring = crate::spring!(Duration::from_millis(300), 1.0);
            if let Err(error) = list.smooth(ui.motion(), spring) {
                ui.fail(error);
            }
            let taps = taps.clone();
            ui.scroll(
                list.clone(),
                Modifier::new().size(100.dp(), 100.dp()),
                move |ui| {
                    for (row, taps) in taps.iter().enumerate() {
                        let taps = taps.clone();
                        ui.scoped(row, Modifier::new(), move |ui| {
                            let taps = taps.clone();
                            ui.spacer(
                                100.dp(),
                                40.dp(),
                                Modifier::new().on_click(move || taps.update(|count| *count += 1)),
                            );
                        });
                    }
                },
            );
        });
    })
}

fn scrolled(offset: Dp) -> Slot<Affine> {
    Slot::Value(Affine::translate(Point::new(Dp::ZERO, -offset)))
}

fn transforms(update: &SceneUpdate) -> Vec<Slot<Affine>> {
    (update.changes.iter())
        .filter_map(|change| match change {
            Change::SetSlot {
                slot: SlotChange::Transform(slot),
                ..
            } => Some(slot.clone()),
            _ => None,
        })
        .collect()
}

/// At the scroll offset of 90 dp, the tap 30 dp below the top of the list lands 120 dp into the
/// content, on row 3.
#[test]
fn jumping_a_scroll_on_every_frame_changes_only_the_content_slot() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = scoped_list(&scroll, &taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let before = view.stats();
    for (ms, offset) in [(10, 30.dp()), (20, 60.dp()), (30, 90.dp())] {
        scroll.jump_to(offset);
        assert!(view.needs_frame(), "{ms} ms");
        let update = frame(&mut view, WINDOW_SIZE, ms)?;
        assert_eq!(update.changes.len(), 1, "{ms} ms");
        assert!(transforms(&update) == [scrolled(offset)], "{ms} ms");
    }
    let after = view.stats();
    assert_eq!(after.painted_components, before.painted_components);
    assert_eq!(after.component_compositions, before.component_compositions);
    tap_at(&mut view, Point::new(50.dp(), 30.dp()));
    assert_eq!(
        taps.iter().map(State::get).collect::<Vec<_>>(),
        [0, 0, 0, 1, 0, 0, 0, 0, 0, 0]
    );
    Ok(())
}

#[test]
fn a_scroll_that_stops_jumping_paints_its_component_once() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = scoped_list(&scroll, &taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll.jump_to(30.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 10)?;
    let painted = view.stats().painted_components;
    assert!(view.needs_frame());
    assert!(frame(&mut view, WINDOW_SIZE, 20)?.changes.is_empty());
    assert_eq!(view.stats().painted_components, painted + 1);
    assert!(!view.needs_frame());
    Ok(())
}

/// A host paints two frames of the view with the same presentation time when the host applies
/// actions between the two frames.
#[test]
fn a_second_frame_at_the_same_time_does_not_settle_a_jumping_scroll() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = scoped_list(&scroll, &taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let painted = view.stats().painted_components;
    for (ms, offset) in [(10, 30.dp()), (20, 60.dp()), (30, 90.dp())] {
        scroll.jump_to(offset);
        advance(&mut scene, &mut view, WINDOW_SIZE, ms)?;
        advance(&mut scene, &mut view, WINDOW_SIZE, ms)?;
    }
    assert_eq!(view.stats().painted_components, painted);
    Ok(())
}

/// A self-test paints frames of the view without presentation times.
#[test]
fn a_jumped_scroll_settles_without_presentation_times() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = scoped_list(&scroll, &taps);
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    advance_with(&mut scene, &mut view, viewport, &mut Metrics)?;
    scroll.jump_to(30.dp());
    advance_with(&mut scene, &mut view, viewport, &mut Metrics)?;
    let painted = view.stats().painted_components;
    advance_with(&mut scene, &mut view, viewport, &mut Metrics)?;
    assert_eq!(view.stats().painted_components, painted + 1);
    assert!(!view.needs_frame());
    Ok(())
}
