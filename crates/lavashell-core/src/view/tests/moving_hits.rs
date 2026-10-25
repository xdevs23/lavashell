// SPDX-License-Identifier: GPL-3.0-only
use super::selection::Shaped;
use super::*;
use crate::animation::{Easing, Tween};
use crate::geometry::FinitePoint;
use crate::scroll::ScrollState;
use std::time::Duration;

fn linear(ms: u64) -> Tween {
    Tween::new(Duration::from_millis(ms))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

const RIGHT: FinitePoint =
    crate::constant!(FinitePoint::try_new(Point::new(Dp::new(200.0), Dp::ZERO)));
const DOWN: FinitePoint =
    crate::constant!(FinitePoint::try_new(Point::new(Dp::ZERO, Dp::new(100.0))));

fn slide(by: FinitePoint) -> Transition {
    Transition::new(linear(1000)).enter(Presence::default().slide(by))
}

fn counts(taps: &[State<u32>]) -> Vec<u32> {
    taps.iter().map(State::get).collect()
}

/// A view with a list 100 dp high that scrolls `scroll` linearly over a second. The list contains
/// one row 40 dp high for each counter in `taps`. Each row is a scoped component that counts its
/// taps in its counter.
fn counted_rows(scroll: &ScrollState, taps: &[State<u32>]) -> View {
    let list = scroll.clone();
    let taps = taps.to_vec();
    View::new(crate::platform::fixture::facts(), move |ui| {
        if let Err(error) = list.smooth(ui.motion(), linear(1000)) {
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
    })
}

fn scroll_by(scene: &mut Scene, view: &mut View, by: Dp) -> TestResult {
    view.input(InputEvent::Scroll {
        at: Point::new(10.dp(), 10.dp()),
        delta: Point::new(Dp::ZERO, by),
    });
    advance(scene, view, WINDOW_SIZE, 0)
}

#[test]
fn a_tap_during_a_smooth_scroll_finds_the_reused_row_where_it_was_shown() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = counted_rows(&scroll, &taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let reused = view.stats().reused_components;
    scroll_by(&mut scene, &mut view, 120.dp())?;
    assert_eq!(view.stats().reused_components, reused + 10);
    view.record_presentation(Duration::from_millis(500));
    tap_at(&mut view, Point::new(50.dp(), 50.dp()));
    assert_eq!(counts(&taps), [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_tap_during_a_smooth_scroll_lands_only_inside_the_viewport_as_shown() -> TestResult {
    let taps: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = counted_rows(&scroll, &taps);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll.jump_to(100.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll_by(&mut scene, &mut view, -80.dp())?;
    view.record_presentation(Duration::from_millis(500));
    tap_at(&mut view, Point::new(50.dp(), 30.dp()));
    tap_at(&mut view, Point::new(50.dp(), 120.dp()));
    assert_eq!(counts(&taps), [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_tap_during_nested_slides_hits_the_card_where_both_show_it() -> TestResult {
    let taps = State::new(0);
    let counted = taps.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let counted = counted.clone();
        ui.scoped(
            "outer",
            Modifier::new().transition(slide(RIGHT)),
            move |ui| {
                let counted = counted.clone();
                ui.scoped(
                    "inner",
                    Modifier::new().transition(slide(DOWN)),
                    move |ui| {
                        let counted = counted.clone();
                        ui.spacer(
                            100.dp(),
                            60.dp(),
                            Modifier::new().on_click(move || counted.update(|count| *count += 1)),
                        );
                    },
                );
            },
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.record_presentation(Duration::from_millis(500));
    for missed in [Point::new(150.dp(), 30.dp()), Point::new(50.dp(), 80.dp())] {
        tap_at(&mut view, missed);
    }
    assert_eq!(taps.get(), 0);
    tap_at(&mut view, Point::new(150.dp(), 80.dp()));
    assert_eq!(taps.get(), 1);
    Ok(())
}

#[test]
fn a_press_during_a_slide_places_the_caret_where_the_text_was_shown() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.text_area(
            "abcdef".to_string(),
            TextFieldOptions::default(),
            Modifier::new()
                .tag("field")
                .size(240.dp(), 60.dp())
                .transition(slide(RIGHT)),
            |_| {},
        );
    });
    let time = FrameTime::default();
    advance_with_timing(
        &mut scene,
        &mut view,
        Viewport::new(WINDOW_SIZE, Density::ONE),
        &mut Shaped,
        time,
    )?;
    view.record_presentation(Duration::from_millis(500));
    view.input(InputEvent::Press(Point::new(131.dp(), 5.dp())));
    let field = view
        .nodes
        .iter()
        .find(|node| node.tag.as_deref() == Some("field"))
        .required("a node tagged field")?;
    assert_eq!(view.edits[&field.id].selection(), 3..3);
    Ok(())
}

#[test]
fn focus_on_a_row_scrolled_out_of_a_smooth_list_falls_back_to_the_list() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let list = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if let Err(error) = list.smooth(ui.motion(), linear(1000)) {
            ui.fail(error);
        }
        ui.scroll(
            list.clone(),
            Modifier::new()
                .tag("list")
                .on_key(|_| false)
                .size(100.dp(), 100.dp()),
            |ui| {
                for row in 0..10 {
                    ui.spacer(
                        100.dp(),
                        40.dp(),
                        Modifier::new().tag(format!("row {row}")).on_click(|| {}),
                    );
                }
            },
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    tap(&mut view, "row 0")?;
    assert!(view.tagged_state("row 0")?.focus.focused);
    scroll.jump_to(300.dp());
    for ms in [10, 20, 30] {
        advance(&mut scene, &mut view, WINDOW_SIZE, ms)?;
    }
    assert!(view.tagged_state("list")?.focus.focused);
    Ok(())
}
