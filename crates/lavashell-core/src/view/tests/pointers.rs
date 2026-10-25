// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

fn describe(pointer: PointerId) -> String {
    match pointer {
        PointerId::Primary => "primary".to_string(),
        PointerId::Touch(touch) => format!("touch {}.{}", touch.seat, touch.point),
    }
}

/// A view with two spacers side by side, tagged "a" and "b", painted once into `scene`. Each spacer
/// appends a line to the returned log for each pointer event and each click sent to it.
fn keys(scene: &mut Scene) -> Result<(View, Log), TestError> {
    let log = Log::default();
    let events = log.clone();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            for tag in ["a", "b"] {
                let (pointer, click) = (events.clone(), events.clone());
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag(tag)
                        .on_pointer(move |input| {
                            let phase = match input.phase {
                                PointerPhase::Pressed => "down",
                                PointerPhase::Moved => "move",
                                PointerPhase::Released => "up",
                                PointerPhase::Cancelled => "cancel",
                            };
                            pointer
                                .borrow_mut()
                                .push(format!("{tag} {} {phase}", describe(input.pointer)));
                        })
                        .on_click(move || click.borrow_mut().push(format!("{tag} click"))),
                );
            }
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok((view, log))
}

fn at(view: &View, tag: &str) -> Result<Point, TestError> {
    Ok(center(view.tagged_bounds(tag)?))
}

#[test]
fn each_touch_point_captures_the_node_it_pressed_and_releases_it_alone() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("a")?.pointer.active && view.tagged_state("b")?.pointer.active);
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    assert!(!view.tagged_state("a")?.pointer.active && view.tagged_state("b")?.pointer.active);
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: Point::new(300.dp(), 200.dp()),
        kind: PointerKind::Finger,
    }));
    let state = view.tagged_state("b")?;
    assert!(state.pointer.captured && !state.pointer.active);
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(2),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *log.borrow(),
        [
            "a touch 0.1 down",
            "b touch 0.2 down",
            "a touch 0.1 up",
            "a click",
            "b touch 0.2 move",
            "b touch 0.2 move",
            "b touch 0.2 up",
            "b click",
        ]
    );
    Ok(())
}

#[test]
fn a_touch_point_ends_at_release_while_the_primary_pointer_hovers_on() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, _) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    view.input(InputEvent::Press(a));
    view.input(InputEvent::Release(a));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(7),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("b")?.pointer.hovered);
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(7),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("a")?.pointer.hovered);
    assert!(!view.tagged_state("b")?.pointer.hovered);
    Ok(())
}

#[test]
fn a_captured_handler_receives_the_end_of_a_press_without_a_release() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::WindowFocus(false));
    view.input(InputEvent::WindowFocus(true));
    view.input(InputEvent::Press(a));
    view.input(InputEvent::CancelPointer);
    view.input(InputEvent::Press(a));
    view.input(InputEvent::Press(b));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *log.borrow(),
        [
            "a touch 0.1 down",
            "a touch 0.1 move",
            "a touch 0.1 cancel",
            "a primary down",
            "a primary cancel",
            "a primary down",
            "a primary cancel",
            "b primary down",
        ]
    );
    Ok(())
}

#[test]
fn removing_or_disabling_a_captured_node_cancels_its_press() -> TestResult {
    let (shown, enabled) = (State::new(true), State::new(true));
    let log = Rc::new(RefCell::new(Vec::new()));
    let (show, enable, events) = (shown.clone(), enabled.clone(), log.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let nodes = [
                ("disabled", true, enable.get()),
                ("removed", show.get(), true),
            ];
            for (tag, _, enabled) in nodes.into_iter().filter(|(_, present, _)| *present) {
                let events = events.clone();
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag(tag)
                        .enabled(enabled)
                        .on_pointer(move |input| {
                            if input.phase == PointerPhase::Cancelled {
                                events.borrow_mut().push((tag, input.position));
                            }
                        }),
                );
            }
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let (disabled, removed) = (at(&view, "disabled")?, at(&view, "removed")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: removed,
        kind: PointerKind::Finger,
    }));
    shown.set(false);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: disabled,
        kind: PointerKind::Finger,
    }));
    enabled.set(false);
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let middle = Point::new(50.dp(), 20.dp());
    assert_eq!(*log.borrow(), [("removed", middle), ("disabled", middle)]);
    Ok(())
}

#[test]
fn a_touch_press_leaves_the_press_of_the_primary_pointer_running() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    view.input(InputEvent::Press(a));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("a")?.pointer.active);
    view.input(InputEvent::Release(a));
    assert_eq!(
        *log.borrow(),
        [
            "a primary down",
            "b touch 0.1 down",
            "b touch 0.1 up",
            "b click",
            "a primary up",
            "a click",
        ]
    );
    Ok(())
}

#[test]
fn touch_points_of_two_seats_with_one_id_press_apart() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    let other = TouchId { seat: 1, point: 0 };
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(0),
        at: a,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: other,
        at: b,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("a")?.pointer.active && view.tagged_state("b")?.pointer.active);
    view.input(InputEvent::TouchUp(TouchSample {
        id: other,
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(0),
        at: a,
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *log.borrow(),
        [
            "a touch 0.0 down",
            "b touch 1.0 down",
            "b touch 1.0 up",
            "b click",
            "a touch 0.0 up",
            "a click",
        ]
    );
    Ok(())
}

#[test]
fn a_cancel_ends_the_press_of_one_pointer_and_leaves_the_others_held() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let (a, b) = (at(&view, "a")?, at(&view, "b")?);
    view.input(InputEvent::Press(a));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchCancel(touch(1)));
    assert!(view.tagged_state("a")?.pointer.active && !view.tagged_state("b")?.pointer.active);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::CancelPointer);
    assert!(!view.tagged_state("a")?.pointer.active && view.tagged_state("b")?.pointer.active);
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: b,
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *log.borrow(),
        [
            "a primary down",
            "b touch 0.1 down",
            "b touch 0.1 cancel",
            "b touch 0.1 down",
            "a primary cancel",
            "b touch 0.1 up",
            "b click",
        ]
    );
    Ok(())
}

#[test]
fn a_touch_point_that_is_not_down_neither_moves_nor_hovers() -> TestResult {
    let mut scene = Scene::default();
    let (mut view, log) = keys(&mut scene)?;
    let a = at(&view, "a")?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchCancel(touch(1)));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: a,
        kind: PointerKind::Finger,
    }));
    assert!(!view.tagged_state("a")?.pointer.hovered);
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: a,
        kind: PointerKind::Finger,
    }));
    assert_eq!(*log.borrow(), ["a touch 0.1 down", "a touch 0.1 cancel"]);
    Ok(())
}
