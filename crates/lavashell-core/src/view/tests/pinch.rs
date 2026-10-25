// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scroll::ScrollState;
use std::cell::RefCell;
use std::rc::Rc;

type Transforms = Rc<RefCell<Vec<TransformInput>>>;

/// A view with a list 100 dp high that scrolls `scroll`, painted once into `scene`. At the top of
/// the list is a spacer of 200 by 100 dp tagged "map" that appends each transform input sent to it
/// to `transforms`. Below the map is a spacer 200 dp high.
fn map_in_list(
    scene: &mut Scene,
    scroll: ScrollState,
    transforms: Transforms,
) -> Result<View, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let transforms = transforms.clone();
        ui.scroll(scroll.clone(), Modifier::new().height(100.dp()), |ui| {
            ui.spacer(
                200.dp(),
                100.dp(),
                Modifier::new()
                    .tag("map")
                    .on_transform(move |input| transforms.borrow_mut().push(input)),
            );
            ui.spacer(200.dp(), 200.dp(), Modifier::new());
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

#[test]
fn a_pinch_in_a_list_scales_around_the_focal_point_of_the_pinch_and_does_not_scroll_the_list()
-> TestResult {
    let (scroll, transforms) = (ScrollState::new(Axis::Vertical), Transforms::default());
    let mut scene = Scene::default();
    let mut view = map_in_list(&mut scene, scroll.clone(), transforms.clone())?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: Point::new(100.dp(), 30.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 70.dp()),
        kind: PointerKind::Finger,
    }));
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 110.dp()),
        kind: PointerKind::Finger,
    }));
    advance(&mut scene, &mut view, WINDOW_SIZE, 33)?;
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: Point::new(100.dp(), 30.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(
        *transforms.borrow(),
        [
            TransformInput {
                focal: Point::new(100.dp(), 50.dp()),
                scale: 1.0,
                phase: DragPhase::Started,
            },
            TransformInput {
                focal: Point::new(100.dp(), 70.dp()),
                scale: 2.0,
                phase: DragPhase::Moved,
            },
            TransformInput {
                focal: Point::new(100.dp(), 70.dp()),
                scale: 2.0,
                phase: DragPhase::Ended,
            },
        ]
    );
    assert_eq!(scroll.offset(), Dp::ZERO);
    Ok(())
}

#[test]
fn a_single_finger_on_a_pinch_in_a_list_scrolls_the_list() -> TestResult {
    let (scroll, transforms) = (ScrollState::new(Axis::Vertical), Transforms::default());
    let mut scene = Scene::default();
    let mut view = map_in_list(&mut scene, scroll.clone(), transforms.clone())?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: Point::new(100.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(1),
        at: Point::new(100.dp(), 20.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 70.dp()),
        kind: PointerKind::Finger,
    }));
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(scroll.offset(), 22.dp());
    assert!(transforms.borrow().is_empty());
    Ok(())
}

#[test]
fn a_node_with_pinch_and_click_handlers_is_not_clicked_after_a_pinch() -> TestResult {
    let (clicks, transforms) = (State::new(0), Transforms::default());
    let (counter, log) = (clicks.clone(), transforms.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (counter, log) = (counter.clone(), log.clone());
        ui.spacer(
            200.dp(),
            100.dp(),
            Modifier::new()
                .tag("map")
                .on_transform(move |input| log.borrow_mut().push(input))
                .on_click(move || counter.update(|count| *count += 1)),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: Point::new(60.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: Point::new(140.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(2),
        at: Point::new(140.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(1),
        at: Point::new(60.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(clicks.get(), 0);
    assert_eq!(transforms.borrow().len(), 2);
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(3),
        at: Point::new(60.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchUp(TouchSample {
        id: touch(3),
        at: Point::new(60.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    assert_eq!(clicks.get(), 1);
    Ok(())
}

#[test]
fn a_started_pinch_that_loses_a_point_is_cancelled_whole() -> TestResult {
    let transforms = Transforms::default();
    let log = transforms.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let log = log.clone();
        ui.spacer(
            200.dp(),
            100.dp(),
            Modifier::new()
                .tag("map")
                .on_transform(move |input| log.borrow_mut().push(input)),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(1),
        at: Point::new(60.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchDown(TouchSample {
        id: touch(2),
        at: Point::new(100.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    view.input(InputEvent::TouchCancel(touch(1)));
    view.input(InputEvent::TouchMove(TouchSample {
        id: touch(2),
        at: Point::new(140.dp(), 50.dp()),
        kind: PointerKind::Finger,
    }));
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    assert_eq!(
        *transforms.borrow(),
        [
            TransformInput {
                focal: Point::new(80.dp(), 50.dp()),
                scale: 1.0,
                phase: DragPhase::Started,
            },
            TransformInput {
                focal: Point::new(80.dp(), 50.dp()),
                scale: 1.0,
                phase: DragPhase::Cancelled,
            },
        ]
    );
    Ok(())
}

#[test]
fn a_new_finger_pairs_with_the_finger_left_from_an_ended_pinch() -> TestResult {
    let transforms = Transforms::default();
    let log = transforms.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let log = log.clone();
        ui.spacer(
            200.dp(),
            100.dp(),
            Modifier::new()
                .tag("map")
                .on_transform(move |input| log.borrow_mut().push(input)),
        );
    });
    advance(&mut scene, &mut view, WINDOW_SIZE, 0)?;
    let finger = |point, x: Dp| TouchSample {
        id: touch(point),
        at: Point::new(x, 50.dp()),
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(finger(1, 60.dp())));
    view.input(InputEvent::TouchDown(finger(2, 100.dp())));
    view.input(InputEvent::TouchMove(finger(2, 140.dp())));
    view.input(InputEvent::TouchUp(finger(2, 140.dp())));
    view.input(InputEvent::TouchDown(finger(3, 100.dp())));
    let phases: Vec<_> = transforms
        .borrow()
        .iter()
        .map(|input| input.phase)
        .collect();
    assert_eq!(
        phases,
        [
            DragPhase::Started,
            DragPhase::Moved,
            DragPhase::Ended,
            DragPhase::Started
        ]
    );
    let restarted = *transforms.borrow().last().required("a transform")?;
    assert_eq!(
        (restarted.focal, restarted.scale),
        (Point::new(80.dp(), 50.dp()), 1.0)
    );
    Ok(())
}

#[test]
fn a_finger_left_from_a_pinch_cancelled_before_it_started_scrolls_the_list() -> TestResult {
    let (scroll, transforms) = (ScrollState::new(Axis::Vertical), Transforms::default());
    let mut scene = Scene::default();
    let mut view = map_in_list(&mut scene, scroll.clone(), transforms.clone())?;
    let finger = |point, y: Dp| TouchSample {
        id: touch(point),
        at: Point::new(100.dp(), y),
        kind: PointerKind::Finger,
    };
    view.input(InputEvent::TouchDown(finger(1, 30.dp())));
    view.input(InputEvent::TouchDown(finger(2, 50.dp())));
    view.input(InputEvent::TouchCancel(touch(1)));
    view.input(InputEvent::TouchMove(finger(2, 20.dp())));
    assert!(transforms.borrow().is_empty());
    assert_eq!(scroll.offset(), 22.dp());
    Ok(())
}
