// SPDX-License-Identifier: GPL-3.0-only
use super::material_motion::painted_fills;
use super::*;
use crate::animation::{Animation, Easing, Tween};
use crate::scroll::ScrollState;
use std::time::Duration;

fn linear(ms: u64) -> Tween {
    Tween::new(Duration::from_millis(ms))
        .easing(Easing::Linear)
        .preserve_velocity(false)
}

/// The rectangle, in screen coordinates, of the only fill drawn in `scene`. The test fails when
/// `scene` draws more or fewer than one fill.
fn shown(scene: &Scene) -> Result<Rect, TestError> {
    let fills = painted_fills(&scene.commands())?;
    assert_eq!(fills.len(), 1);
    Ok(fills[0].rect)
}

/// A white node of 60 by 40 dp, shared and tagged as "shared", that moves linearly over a second to
/// each new place where it is laid out.
fn shared(ui: &mut Ui) {
    ui.shared(
        "shared",
        Modifier::new()
            .background(Color::WHITE)
            .transition(linear(1000))
            .tag("shared"),
        |ui| {
            ui.spacer(60.dp(), 40.dp(), Modifier::new());
        },
    );
}

/// A column 20 dp down and 10 dp to the right, or 300 dp to the right when `right` is true. The
/// column contains the nodes added by `content` only while `filled` is true.
fn column(ui: &mut Ui, right: bool, filled: bool, content: fn(&mut Ui)) {
    ui.layout(
        Axis::Vertical,
        Modifier::new().offset(if right { 300.dp() } else { 10.dp() }, 20.dp()),
        move |ui| {
            if filled {
                content(ui);
            }
        },
    );
}

#[test]
fn a_shared_node_in_a_sliding_parent_ends_where_it_is_laid_out() -> TestResult {
    let right = State::new(false);
    let moved = right.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let holder = moved.get();
        let slide = Presence::default().slide(crate::constant!(slide_offset(200.0, 0.0)));
        ui.layout(
            Axis::Vertical,
            Modifier::new().transition(Transition::new(linear(1000)).enter(slide)),
            move |ui| {
                for right in [false, true] {
                    column(ui, right, holder == right, shared);
                }
            },
        );
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 500)?;
    assert!(!view.needs_frame());
    assert_eq!(view.next_deadline(), Some(Duration::from_secs(1)));
    let before = shown(scene.at(1000))?;
    let rest = frame(&mut view, WINDOW_SIZE, 1000)?;
    assert_eq!(rest.changes.len(), 1, "only the slot of the parent changes");
    scene.show(rest)?;
    assert_eq!(shown(&scene)?, before);
    let before = shown(scene.at(1500))?;
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1500)?;
    assert_eq!(shown(&scene)?, before);
    assert_eq!(view.bounds_of("shared"), Some(before));
    assert_eq!(before.x, 300.dp());
    Ok(())
}

/// A spacer 30 dp high, then the node from `shared` inside a scoped component keyed "component". In
/// a column, this places the component 30 dp below the top of the column.
fn in_component(ui: &mut Ui) {
    ui.spacer(10.dp(), 30.dp(), Modifier::new());
    ui.scoped("component", Modifier::new(), shared);
}

#[test]
fn a_shared_node_in_a_component_moves_between_the_places_where_it_is_laid_out() -> TestResult {
    let right = State::new(false);
    let moved = right.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let holder = moved.get();
        for right in [false, true] {
            column(ui, right, holder == right, in_component);
        }
    });
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = shown(&scene)?;
    assert_eq!(view.bounds_of("shared"), Some(start));
    assert_eq!((start.x, start.y), (10.dp(), 50.dp()));
    right.set(true);
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    assert_eq!(shown(&scene)?, start);
    assert_eq!(shown(scene.at(500))?.x, 155.dp());
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 1000)?;
    let end = shown(&scene)?;
    assert_eq!(view.bounds_of("shared"), Some(end));
    assert_eq!(end.x, 300.dp());
    Ok(())
}

#[test]
fn a_shared_node_in_moved_content_shows_where_it_is_laid_out() -> TestResult {
    fn content(ui: &mut Ui) {
        ui.spacer(100.dp(), 100.dp(), Modifier::new());
        shared(ui);
        ui.spacer(100.dp(), 400.dp(), Modifier::new());
    }
    let scroll = ScrollState::new(Axis::Vertical);
    let list = scroll.clone();
    let scrolled = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.scroll(
            list.clone(),
            Modifier::new().size(100.dp(), 200.dp()),
            content,
        );
    });
    let offset = Animation::new(Point::new(Dp::ZERO, -50.dp()))?;
    let moving = offset.clone();
    let offset_by = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(
            Axis::Vertical,
            Modifier::new().animated_offset(moving.clone()),
            content,
        );
    });
    for (case, mut view) in [("scroll", scrolled), ("offset", offset_by)] {
        let mut scene = Scene::default();
        scroll.jump_to(50.dp());
        show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
        assert_eq!(shown(&scene)?.y, 50.dp(), "{case}");
    }
    Ok(())
}

/// The host grows the window of the view 300 dp upwards over the screen and moves its viewport by
/// as much, while the content does not move on the screen.
#[test]
fn a_shared_node_does_not_move_on_the_screen_while_the_host_moves_the_viewport_with_the_content()
-> TestResult {
    let grown = State::new(false);
    let read = grown.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let top = if read.get() { 300.dp() } else { 0.dp() };
        ui.layout(
            Axis::Vertical,
            Modifier::new().offset(10.dp(), 20.dp() + top),
            shared,
        );
    });
    view.set_viewport_origin(Point::new(0.dp(), 300.dp()));
    let mut scene = Scene::default();
    show_frame(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    grown.set(true);
    view.set_viewport_origin(Point::default());
    let window = Size::new(WINDOW_SIZE.width, WINDOW_SIZE.height + 300.dp());
    show_frame(&mut scene, &mut view, window, 0)?;
    let rest = view.bounds_of("shared");
    assert_eq!(rest, Some(shown(scene.at(500))?));
    Ok(())
}
