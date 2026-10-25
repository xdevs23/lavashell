// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scroll::ScrollState;
use std::cell::RefCell;
use std::rc::Rc;

#[path = "gestures/fingers.rs"]
mod fingers;

/// A view with a list 100 dp high that scrolls `scroll`, painted once into `scene`. The list
/// contains a spacer tagged "slider" that appends the phase of each horizontal drag to `drags`,
/// then five spacers 40 dp high tagged "tab 0" to "tab 4" that count their clicks in `clicks`.
fn list(
    scene: &mut Scene,
    scroll: ScrollState,
    clicks: State<u32>,
    drags: Rc<RefCell<Vec<DragPhase>>>,
) -> Result<View, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (clicks, drags) = (clicks.clone(), drags.clone());
        ui.scroll(scroll.clone(), Modifier::new().height(100.dp()), |ui| {
            let drag = move |input: DragInput| drags.borrow_mut().push(input.phase);
            let slider = Modifier::new()
                .tag("slider")
                .on_drag(Axis::Horizontal, drag);
            ui.spacer(200.dp(), 40.dp(), slider);
            for index in 0..5 {
                let clicks = clicks.clone();
                let tab = Modifier::new().tag(format!("tab {index}"));
                let tab = tab.on_click(move || clicks.update(|count| *count += 1));
                ui.spacer(200.dp(), 40.dp(), tab);
            }
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

#[test]
fn a_tab_dragged_along_the_list_containing_it_scrolls_the_list_past_the_slop_and_does_not_click()
-> TestResult {
    let (scroll, clicks) = (ScrollState::new(Axis::Vertical), State::new(0));
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), clicks.clone(), Rc::default())?;
    let start = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, -4.dp()),
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("tab 0")?.pointer.active);
    assert_eq!(scroll.offset(), Dp::ZERO);
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, -30.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 22.dp());
    assert!(!view.tagged_state("tab 0")?.pointer.active);
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, -50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, -50.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 42.dp());
    assert_eq!(clicks.get(), 0);
    Ok(())
}

#[test]
fn a_list_drawn_at_half_size_scrolls_by_twice_the_movement_of_the_finger() -> TestResult {
    const HALF: Positive = crate::constant!(Positive::try_new(0.5));
    let scroll = ScrollState::new(Axis::Vertical);
    let state = scroll.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let state = state.clone();
        let half = Modifier::new().visual_scale(HALF);
        ui.layout(Arrangement::Overlay, half, |ui| {
            ui.scroll(state, Modifier::new().height(100.dp()), |ui| {
                for index in 0..5 {
                    let row = Modifier::new().tag(format!("row {index}"));
                    ui.spacer(200.dp(), 40.dp(), row);
                }
            });
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = center(view.tagged_bounds("row 0")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, -18.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 20.dp());
    Ok(())
}

#[test]
fn a_diagonal_move_that_rejects_a_tap_scrolls_the_list_only_past_the_slop() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), State::new(0), Rc::default())?;
    let start = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(7.dp(), -5.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), Dp::ZERO);
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(7.dp(), -30.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 22.dp());
    Ok(())
}

#[test]
fn a_touch_press_that_does_not_move_clicks_and_takes_focus_at_the_release() -> TestResult {
    let (scroll, clicks) = (ScrollState::new(Axis::Vertical), State::new(0));
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), clicks.clone(), Rc::default())?;
    let start = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(3.dp(), 3.dp()),
        kind: PointerKind::Finger,
    }));
    assert!(!view.tagged_state("tab 0")?.focus.focused);
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start.offset(3.dp(), 3.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(clicks.get(), 1);
    assert!(view.tagged_state("tab 0")?.focus.focused);
    assert_eq!(scroll.offset(), Dp::ZERO);
    Ok(())
}

#[test]
fn a_tab_that_wins_a_finger_after_a_key_takes_focus_from_the_pointer() -> TestResult {
    let mut scene = Scene::default();
    let mut view = list(
        &mut scene,
        ScrollState::new(Axis::Vertical),
        State::new(0),
        Rc::default(),
    )?;
    let start = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::Key {
        key: Key::Character('a'),
        shift: false,
    });
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    let state = view.tagged_state("tab 0")?;
    assert!(state.focus.focused && !state.focus.visible);
    Ok(())
}

#[test]
fn a_touch_press_that_does_not_move_focuses_a_node_without_a_click_inside_a_list() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scroll(Axis::Vertical, Modifier::new().height(100.dp()), |ui| {
            let keys = Modifier::new().tag("keys").on_key(|_| false);
            ui.spacer(200.dp(), 40.dp(), keys);
            ui.spacer(200.dp(), 200.dp(), Modifier::new());
        });
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = center(view.tagged_bounds("keys")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    assert!(view.tagged_state("keys")?.focus.focused);
    Ok(())
}

#[test]
fn a_release_beside_a_pressed_button_clicks_neither_the_button_nor_the_card_around_it() -> TestResult
{
    let (button, card) = (State::new(0), State::new(0));
    let (pressed, around) = (button.clone(), card.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (pressed, around) = (pressed.clone(), around.clone());
        ui.layout(
            Arrangement::Overlay,
            Modifier::new()
                .on_click(move || around.update(|count| *count += 1))
                .size(200.dp(), 100.dp()),
            |ui| {
                ui.spacer(
                    100.dp(),
                    40.dp(),
                    Modifier::new()
                        .tag("button")
                        .on_click(move || pressed.update(|count| *count += 1)),
                );
            },
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let edge = view.tagged_bounds("button")?.width - 2.dp();
    let (inside, beside) = (
        Point::new(edge, 20.dp()),
        Point::new(edge + 4.dp(), 20.dp()),
    );
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: inside,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: beside,
        kind: PointerKind::Finger,
    }));
    assert_eq!((button.get(), card.get()), (0, 0));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: inside,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(2),
        at: inside,
        kind: PointerKind::Finger,
    }));
    assert_eq!((button.get(), card.get()), (1, 0));
    Ok(())
}

#[test]
fn a_drag_inside_a_list_receives_a_diagonal_move_and_not_a_move_along_the_list() -> TestResult {
    let (scroll, drags) = (
        ScrollState::new(Axis::Vertical),
        Rc::new(RefCell::new(Vec::new())),
    );
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), State::new(0), drags.clone())?;
    let start = center(view.tagged_bounds("slider")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(30.dp(), -30.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(40.dp(), -60.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start.offset(40.dp(), -60.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *drags.borrow(),
        [DragPhase::Started, DragPhase::Moved, DragPhase::Ended]
    );
    assert_eq!(scroll.offset(), Dp::ZERO);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: start.offset(2.dp(), -20.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 12.dp());
    assert_eq!(drags.borrow().len(), 3);
    Ok(())
}

#[test]
fn a_flick_mostly_along_the_list_that_starts_on_a_sideways_drag_scrolls_the_list() -> TestResult {
    let (scroll, drags) = (
        ScrollState::new(Axis::Vertical),
        Rc::new(RefCell::new(Vec::new())),
    );
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), State::new(0), drags.clone())?;
    let start = center(view.tagged_bounds("slider")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(9.dp(), -30.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(scroll.offset(), 22.dp());
    assert!(drags.borrow().is_empty());
    Ok(())
}

#[test]
fn a_node_with_drag_and_click_handlers_is_clicked_only_by_a_press_that_does_not_move() -> TestResult
{
    let (clicks, drags) = (State::new(0), Rc::new(RefCell::new(Vec::new())));
    let (counter, log) = (clicks.clone(), drags.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (counter, log) = (counter.clone(), log.clone());
        let card = Modifier::new()
            .tag("card")
            .on_drag(Axis::Horizontal, move |input| {
                log.borrow_mut().push(input.phase);
            })
            .on_click(move || counter.update(|count| *count += 1));
        ui.spacer(200.dp(), 40.dp(), card);
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = center(view.tagged_bounds("card")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(20.dp(), Dp::ZERO),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start.offset(20.dp(), Dp::ZERO),
        kind: PointerKind::Finger,
    }));
    assert_eq!(clicks.get(), 0);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(2),
        at: start,
        kind: PointerKind::Finger,
    }));
    assert_eq!(clicks.get(), 1);
    assert_eq!(*drags.borrow(), [DragPhase::Started, DragPhase::Ended]);
    Ok(())
}

#[test]
fn a_mouse_drag_on_a_list_moves_no_content_and_clicks_where_it_is_released() -> TestResult {
    let (scroll, clicks) = (ScrollState::new(Axis::Vertical), State::new(0));
    let mut scene = Scene::default();
    let mut view = list(&mut scene, scroll.clone(), clicks.clone(), Rc::default())?;
    let start = center(view.tagged_bounds("tab 0")?);
    view.input(InputEvent::Press(start));
    view.input(InputEvent::Move(start.offset(Dp::ZERO, -15.dp())));
    view.input(InputEvent::Release(start.offset(Dp::ZERO, -15.dp())));
    assert_eq!(scroll.offset(), Dp::ZERO);
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn removing_a_node_cancels_its_drag() -> TestResult {
    let (shown, drags) = (State::new(true), Rc::new(RefCell::new(Vec::new())));
    let (show, log) = (shown.clone(), drags.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        if show.get() {
            let log = log.clone();
            let drag = move |input: DragInput| log.borrow_mut().push(input.phase);
            let handle = Modifier::new().tag("handle").on_drag(Axis::Vertical, drag);
            ui.spacer(200.dp(), 40.dp(), handle);
        }
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let start = center(view.tagged_bounds("handle")?);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: start,
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, 20.dp()),
        kind: PointerKind::Finger,
    }));
    shown.set(false);
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: start.offset(Dp::ZERO, 20.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *drags.borrow(),
        [DragPhase::Started, DragPhase::Moved, DragPhase::Cancelled]
    );
    Ok(())
}
