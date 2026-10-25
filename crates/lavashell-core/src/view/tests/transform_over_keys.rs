// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

type Transforms = Rc<RefCell<Vec<TransformInput>>>;
type Presses = Rc<RefCell<Vec<(&'static str, PointerPhase)>>>;

/// A row with `on_transform` that appends each transform input to `transforms`, painted once into
/// `scene`. The row contains the key d at x 0 to 100 and the key k at x 200 to 300, which both
/// append their pointer phases to `presses`.
fn keys(scene: &mut Scene, transforms: Transforms, presses: Presses) -> Result<View, TestError> {
    transforming_keys(scene, transforms, presses, State::new(true))
}

/// The row of `keys`, which declares `on_transform` while `transforming` is true.
fn transforming_keys(
    scene: &mut Scene,
    transforms: Transforms,
    presses: Presses,
    transforming: State<bool>,
) -> Result<View, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let mut row = Modifier::new().gap(100.dp());
        if transforming.get() {
            let transforms = transforms.clone();
            row = row.on_transform(move |input| transforms.borrow_mut().push(input));
        }
        ui.layout(Axis::Horizontal, row, |ui| {
            for key in ["d", "k"] {
                let presses = presses.clone();
                let logged =
                    move |input: PointerInput| presses.borrow_mut().push((key, input.phase));
                ui.spacer(100.dp(), 100.dp(), Modifier::new().on_pointer(logged));
            }
        });
    });
    advance(scene, &mut view, WINDOW_SIZE, 0)?;
    Ok(view)
}

fn finger(point: i32, x: Dp) -> TouchSample {
    TouchSample {
        id: touch(point),
        at: Point::new(x, 50.dp()),
        kind: PointerKind::Finger,
    }
}

fn phases_and_scales(transforms: &Transforms) -> Vec<(DragPhase, f64)> {
    (transforms.borrow().iter())
        .map(|input| (input.phase, input.scale))
        .collect()
}

#[test]
fn two_fingers_on_two_keys_that_stay_apart_type_on_both_keys() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 250.dp())));
    view.input(InputEvent::TouchMove(finger(2, 246.dp())));
    view.input(InputEvent::TouchUp(finger(1, 50.dp())));
    view.input(InputEvent::TouchUp(finger(2, 246.dp())));
    assert!(transforms.borrow().is_empty());
    assert_eq!(
        *presses.borrow(),
        [
            ("d", PointerPhase::Pressed),
            ("k", PointerPhase::Pressed),
            ("k", PointerPhase::Moved),
            ("d", PointerPhase::Released),
            ("k", PointerPhase::Released),
        ]
    );
    Ok(())
}

#[test]
fn two_fingers_on_two_keys_that_move_together_cancel_the_keys_and_report_the_scale() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 280.dp())));
    view.input(InputEvent::TouchMove(finger(2, 250.dp())));
    view.input(InputEvent::TouchMove(finger(2, 150.dp())));
    view.input(InputEvent::TouchUp(finger(2, 150.dp())));
    view.input(InputEvent::TouchUp(finger(1, 50.dp())));
    assert_eq!(
        *transforms.borrow(),
        [
            TransformInput {
                focal: Point::new(150.dp(), 50.dp()),
                scale: 1.0,
                phase: DragPhase::Started,
            },
            TransformInput {
                focal: Point::new(100.dp(), 50.dp()),
                scale: 0.5,
                phase: DragPhase::Moved,
            },
            TransformInput {
                focal: Point::new(100.dp(), 50.dp()),
                scale: 0.5,
                phase: DragPhase::Ended,
            },
        ]
    );
    assert_eq!(
        *presses.borrow(),
        [
            ("d", PointerPhase::Pressed),
            ("k", PointerPhase::Pressed),
            ("k", PointerPhase::Cancelled),
            ("d", PointerPhase::Cancelled),
        ]
    );
    Ok(())
}

#[test]
fn two_fingers_on_two_keys_that_move_apart_cancel_the_keys_and_report_the_scale() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 220.dp())));
    view.input(InputEvent::TouchMove(finger(2, 250.dp())));
    view.input(InputEvent::TouchMove(finger(2, 290.dp())));
    assert_eq!(
        phases_and_scales(&transforms),
        [(DragPhase::Started, 1.0), (DragPhase::Moved, 1.2)]
    );
    assert_eq!(
        *presses.borrow(),
        [
            ("d", PointerPhase::Pressed),
            ("k", PointerPhase::Pressed),
            ("k", PointerPhase::Cancelled),
            ("d", PointerPhase::Cancelled),
        ]
    );
    Ok(())
}

#[test]
fn a_single_finger_on_a_key_never_starts_the_transform() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchMove(finger(1, 90.dp())));
    view.input(InputEvent::TouchMove(finger(1, 10.dp())));
    view.input(InputEvent::TouchUp(finger(1, 10.dp())));
    assert!(transforms.borrow().is_empty());
    assert_eq!(
        *presses.borrow(),
        [
            ("d", PointerPhase::Pressed),
            ("d", PointerPhase::Moved),
            ("d", PointerPhase::Moved),
            ("d", PointerPhase::Released),
        ]
    );
    Ok(())
}

#[test]
fn a_finger_beside_the_keys_takes_the_key_of_the_other_finger_past_the_slop() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 150.dp())));
    view.input(InputEvent::TouchMove(finger(2, 145.dp())));
    assert!(transforms.borrow().is_empty());
    view.input(InputEvent::TouchMove(finger(2, 130.dp())));
    assert_eq!(phases_and_scales(&transforms), [(DragPhase::Started, 1.0)]);
    assert_eq!(
        *presses.borrow(),
        [("d", PointerPhase::Pressed), ("d", PointerPhase::Cancelled)]
    );
    Ok(())
}

#[test]
fn a_finger_lifted_from_a_key_leaves_the_transform_and_the_other_key_keeps_its_finger() -> TestResult
{
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let mut scene = Scene::default();
    let mut view = keys(&mut scene, transforms.clone(), presses.clone())?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 250.dp())));
    view.input(InputEvent::TouchUp(finger(1, 50.dp())));
    view.input(InputEvent::TouchMove(finger(2, 150.dp())));
    assert!(transforms.borrow().is_empty());
    assert_eq!(presses.borrow().last(), Some(&("k", PointerPhase::Moved)));
    Ok(())
}

#[test]
fn a_node_composed_again_without_on_transform_leaves_the_keys_their_fingers() -> TestResult {
    let (transforms, presses) = (Transforms::default(), Presses::default());
    let transforming = State::new(true);
    let mut scene = Scene::default();
    let mut view = transforming_keys(
        &mut scene,
        transforms.clone(),
        presses.clone(),
        transforming.clone(),
    )?;
    view.input(InputEvent::TouchDown(finger(1, 50.dp())));
    view.input(InputEvent::TouchDown(finger(2, 250.dp())));
    transforming.set(false);
    advance(&mut scene, &mut view, WINDOW_SIZE, 16)?;
    view.input(InputEvent::TouchMove(finger(2, 150.dp())));
    view.input(InputEvent::TouchUp(finger(2, 150.dp())));
    assert!(transforms.borrow().is_empty());
    assert_eq!(
        *presses.borrow(),
        [
            ("d", PointerPhase::Pressed),
            ("k", PointerPhase::Pressed),
            ("k", PointerPhase::Moved),
            ("k", PointerPhase::Released),
        ]
    );
    Ok(())
}
