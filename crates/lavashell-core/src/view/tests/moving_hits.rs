// SPDX-License-Identifier: GPL-3.0-only
use super::selection::{Shaped, field_id, padded_field};
use super::*;
use crate::animation::{Easing, Tween};
use crate::frame::FrameTime;
use crate::geometry::{Axis, FinitePoint, Viewport};
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

fn counts(activations: &[State<u32>]) -> Vec<u32> {
    activations.iter().map(State::get).collect()
}

fn counted_rows(scroll: &ScrollState, activations: &[State<u32>]) -> View {
    let list = scroll.clone();
    let activations = activations.to_vec();
    View::new(crate::platform::fixture::facts(), move |ui| {
        if let Err(error) = list.smooth(ui.motion(), linear(1000)) {
            ui.fail(error);
        }
        let activations = activations.clone();
        ui.scroll(
            list.clone(),
            Modifier::new().size(100.dp(), 100.dp()),
            move |ui| {
                for (row, activations) in activations.iter().enumerate() {
                    let activations = activations.clone();
                    ui.scoped(row, Modifier::new(), move |ui| {
                        let activations = activations.clone();
                        ui.spacer(
                            100.dp(),
                            40.dp(),
                            Modifier::new()
                                .on_activate(move |_| activations.update(|count| *count += 1)),
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
fn a_press_during_a_smooth_scroll_finds_the_reused_row_where_it_was_shown() -> TestResult {
    let activations: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = counted_rows(&scroll, &activations);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let reused = view.stats().reused_components;
    scroll_by(&mut scene, &mut view, 120.dp())?;
    assert_eq!(view.stats().reused_components, reused + 10);
    view.record_presentation(Duration::from_millis(500));
    press_and_release_at(&mut view, Point::new(50.dp(), 50.dp()));
    assert_eq!(counts(&activations), [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_press_during_a_smooth_scroll_lands_only_inside_the_viewport_as_shown() -> TestResult {
    let activations: Vec<_> = (0..10).map(|_| State::new(0)).collect();
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = counted_rows(&scroll, &activations);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll.jump_to(100.dp());
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    scroll_by(&mut scene, &mut view, -80.dp())?;
    view.record_presentation(Duration::from_millis(500));
    press_and_release_at(&mut view, Point::new(50.dp(), 30.dp()));
    press_and_release_at(&mut view, Point::new(50.dp(), 120.dp()));
    assert_eq!(counts(&activations), [0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn a_press_during_nested_slides_hits_the_card_where_both_show_it() -> TestResult {
    let activations = State::new(0);
    let counted = activations.clone();
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
                            Modifier::new()
                                .on_activate(move |_| counted.update(|count| *count += 1)),
                        );
                    },
                );
            },
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.record_presentation(Duration::from_millis(500));
    for missed in [Point::new(150.dp(), 30.dp()), Point::new(50.dp(), 80.dp())] {
        press_and_release_at(&mut view, missed);
    }
    assert_eq!(activations.get(), 0);
    press_and_release_at(&mut view, Point::new(150.dp(), 80.dp()));
    assert_eq!(activations.get(), 1);
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
        .find(|node| node.has_tag(&"field"))
        .required("a node tagged field")?;
    assert_eq!(view.editing.edits[&field.id].selection(), 3..3);
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
                        Modifier::new().tag(("row", row)).on_activate(|_| {}),
                    );
                }
            },
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    press_and_release(&mut view, &("row", 0))?;
    assert!(view.tagged_state(&("row", 0))?.focus.focused);
    scroll.jump_to(300.dp());
    for ms in [10, 20, 30] {
        advance(&mut scene, &mut view, WINDOW_SIZE, ms)?;
    }
    assert!(view.tagged_state(&"list")?.focus.focused);
    Ok(())
}

#[test]
fn a_press_on_a_padded_field_in_a_reused_row_finds_the_text_where_it_is_shown() -> TestResult {
    let scroll = ScrollState::new(Axis::Horizontal);
    let row = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(row.clone(), Modifier::new().size(200.dp(), 60.dp()), |ui| {
            ui.spacer(150.dp(), 60.dp(), Modifier::new());
            ui.scoped("row", Modifier::new(), |ui| {
                padded_field(ui, Modifier::new());
            });
        });
    });
    let window = Viewport::new(WINDOW_SIZE, Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    let reused = view.stats().reused_components;
    scroll.jump_to(40.dp());
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    assert!(view.stats().reused_components > reused);
    let id = field_id(&view)?;
    press_and_release_at(&mut view, Point::new(114.dp(), 26.dp()));
    assert_eq!(view.focus(), Some(id));
    assert_eq!(view.editing.edits[&id].selection(), 0..0);
    press_and_release_at(&mut view, Point::new(137.dp(), 26.dp()));
    assert_eq!(view.editing.edits[&id].selection(), 1..1);
    Ok(())
}

#[test]
fn a_press_on_a_field_that_follows_a_scroll_lands_where_the_field_is_shown() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let list = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let list = list.clone();
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            ui.scroll(
                list.clone(),
                Modifier::new().size(100.dp(), 100.dp()),
                |ui| {
                    ui.spacer(100.dp(), 200.dp(), Modifier::new());
                },
            );
            padded_field(ui, Modifier::new().follow_scroll(list, 50.dp()));
        });
    });
    let window = Viewport::new(WINDOW_SIZE, Density::ONE);
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    scroll.jump_to(100.dp());
    advance_with(&mut scene, &mut view, window, &mut Shaped)?;
    let id = field_id(&view)?;
    press_and_release_at(&mut view, Point::new(60.dp(), 126.dp()));
    assert_eq!(view.focus(), None);
    press_and_release_at(&mut view, Point::new(4.dp(), 176.dp()));
    assert_eq!(view.focus(), Some(id));
    assert_eq!(view.editing.edits[&id].selection(), 0..0);
    Ok(())
}
