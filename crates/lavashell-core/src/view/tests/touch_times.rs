// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::geometry::Axis;
use crate::ui::{DragInput, DragPhase, PointerInput, PointerPhase};
use crate::view::fixture::{input_time_at_millis, touch_move_at};
use std::cell::RefCell;
use std::rc::Rc;

type Received<T> = Rc<RefCell<Vec<T>>>;

fn handlers(
    scene: &mut Scene,
    drags: &Received<DragInput>,
    pointers: &Received<PointerInput>,
) -> Result<View, TestError> {
    let (drags, pointers) = (drags.clone(), pointers.clone());
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (drags, pointers) = (drags.clone(), pointers.clone());
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            let (drags, pointers) = (drags.clone(), pointers.clone());
            let dragged = Modifier::new()
                .tag("dragged")
                .on_drag(Axis::Horizontal, move |input| {
                    drags.borrow_mut().push(input);
                });
            ui.spacer(200.dp(), 40.dp(), dragged);
            let followed = Modifier::new()
                .tag("followed")
                .on_pointer(move |input| pointers.borrow_mut().push(input));
            ui.spacer(200.dp(), 40.dp(), followed);
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

fn move_touch_along(view: &mut View, tag: &'static str) -> TestResult {
    let start = view.tagged_center(&tag)?;
    let end = start.offset(60.dp(), Dp::ZERO);
    view.input(InputEvent::TouchDown(finger(1, start)));
    view.input(InputEvent::TouchMove(touch_move_at(
        finger(1, start.offset(30.dp(), Dp::ZERO)),
        8,
    )));
    view.input(InputEvent::TouchMove(touch_move_at(finger(1, end), 16)));
    view.input(InputEvent::TouchUp(finger(1, end)));
    Ok(())
}

#[test]
fn a_drag_handler_receives_the_time_each_touch_position_was_taken() -> TestResult {
    let (mut scene, drags) = (Scene::default(), Received::default());
    let mut view = handlers(&mut scene, &drags, &Received::default())?;
    move_touch_along(&mut view, "dragged")?;
    let received: Vec<_> = (drags.borrow().iter())
        .map(|input| (input.phase, input.time))
        .collect();
    let expected = [
        (DragPhase::Started, None),
        (DragPhase::Moved, Some(input_time_at_millis(8))),
        (DragPhase::Moved, Some(input_time_at_millis(16))),
        (DragPhase::Ended, None),
    ];
    assert_eq!(received, expected);
    Ok(())
}

#[test]
fn a_pointer_handler_receives_the_time_each_touch_position_was_taken() -> TestResult {
    let (mut scene, pointers) = (Scene::default(), Received::default());
    let mut view = handlers(&mut scene, &Received::default(), &pointers)?;
    move_touch_along(&mut view, "followed")?;
    let received: Vec<_> = (pointers.borrow().iter())
        .map(|input| (input.phase, input.time))
        .collect();
    let expected = [
        (PointerPhase::Pressed, None),
        (PointerPhase::Moved, Some(input_time_at_millis(8))),
        (PointerPhase::Moved, Some(input_time_at_millis(16))),
        (PointerPhase::Released, None),
    ];
    assert_eq!(received, expected);
    Ok(())
}
