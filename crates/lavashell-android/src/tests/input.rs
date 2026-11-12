// SPDX-License-Identifier: GPL-3.0-only
use super::motion::{finger, holding, hovering, mouse, pen, press, release, touch};
use crate::input::{MOUSE, MotionEvent, MotionTranslator, MouseAction, ShownSurface, TouchAction};
use lavashell_core::geometry::{Point, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::units::{Density, DpExt, NumberError};
use lavashell_core::view::TouchSample;
use lavashell_platform::{Button, DeviceEvent, InputSerial, PressState};

const SURFACE: SurfaceId = SurfaceId(1);
/// The device ids of a touch screen and of a drawing tablet.
const SCREEN: u32 = 3;
const TABLET: u32 = 5;

/// `SURFACE` shown in a Surface of 100 × 100 dp at `density`.
fn shown(density: Density) -> ShownSurface {
    ShownSurface {
        id: SURFACE,
        size: Size::new(100.dp(), 100.dp()),
        density,
    }
}

/// The device events of `recorded` in order, on `SURFACE` shown at `density`.
fn translate(recorded: Vec<MotionEvent>, density: Density) -> Vec<DeviceEvent> {
    let mut translator = MotionTranslator::default();
    (recorded.into_iter())
        .flat_map(|event| translator.translate(event, Some(shown(density))))
        .collect()
}

fn sample(seat: u32, point: i32, kind: PointerKind, at: Point) -> TouchSample {
    TouchSample {
        id: TouchId { seat, point },
        at,
        kind,
    }
}

fn on_screen(point: i32) -> TouchId {
    TouchId {
        seat: SCREEN,
        point,
    }
}

fn finger_at(point: i32, x: f64, y: f64) -> TouchSample {
    sample(SCREEN, point, PointerKind::Finger, at(x, y))
}

fn at(x: f64, y: f64) -> Point {
    Point::new(x.dp(), y.dp())
}

fn enter(x: f64, y: f64) -> DeviceEvent {
    DeviceEvent::PointerEnter {
        pointer: MOUSE,
        surface: SURFACE,
        at: at(x, y),
    }
}

fn motion(x: f64, y: f64) -> DeviceEvent {
    DeviceEvent::PointerMotion {
        pointer: MOUSE,
        at: at(x, y),
    }
}

fn leave() -> DeviceEvent {
    DeviceEvent::PointerLeave { pointer: MOUSE }
}

fn button(button: Button, state: PressState, at: Point, serial: u64) -> DeviceEvent {
    DeviceEvent::Button {
        pointer: MOUSE,
        button,
        state,
        at,
        serial: InputSerial(serial),
    }
}

#[test]
fn a_tap_goes_down_moves_and_goes_up_at_its_dp_position() -> Result<(), NumberError> {
    let recorded = vec![
        touch(SCREEN, TouchAction::Down(finger(0, 20.0, 40.0))),
        touch(SCREEN, TouchAction::Move(vec![finger(0, 24.0, 42.0)])),
        touch(SCREEN, TouchAction::Up(finger(0, 24.0, 42.0))),
    ];
    let expected = [
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(0, 10.0, 20.0),
        },
        DeviceEvent::TouchMotion(finger_at(0, 12.0, 21.0)),
        DeviceEvent::TouchUp(finger_at(0, 12.0, 21.0)),
    ];
    assert_eq!(
        translate(recorded, Density::from_ratio(2.0, 1.0)?),
        expected
    );
    Ok(())
}

#[test]
fn a_two_finger_gesture_lifts_the_first_finger_while_the_second_moves_on() {
    let both = vec![finger(0, 12.0, 10.0), finger(1, 48.0, 10.0)];
    let recorded = vec![
        touch(SCREEN, TouchAction::Down(finger(0, 10.0, 10.0))),
        touch(SCREEN, TouchAction::Down(finger(1, 50.0, 10.0))),
        touch(SCREEN, TouchAction::Move(both)),
        touch(SCREEN, TouchAction::Up(finger(0, 12.0, 10.0))),
        touch(SCREEN, TouchAction::Up(finger(1, 48.0, 10.0))),
    ];
    let expected = [
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(0, 10.0, 10.0),
        },
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(1, 50.0, 10.0),
        },
        DeviceEvent::TouchMotion(finger_at(0, 12.0, 10.0)),
        DeviceEvent::TouchMotion(finger_at(1, 48.0, 10.0)),
        DeviceEvent::TouchUp(finger_at(0, 12.0, 10.0)),
        DeviceEvent::TouchUp(finger_at(1, 48.0, 10.0)),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn a_cancel_ends_every_finger_that_is_down_and_a_pointer_cancel_ends_one() {
    let recorded = vec![
        touch(SCREEN, TouchAction::Down(finger(0, 10.0, 10.0))),
        touch(SCREEN, TouchAction::Down(finger(1, 50.0, 10.0))),
        touch(SCREEN, TouchAction::Down(finger(2, 90.0, 10.0))),
        touch(SCREEN, TouchAction::Cancel(vec![2])),
        touch(SCREEN, TouchAction::Cancel(vec![0, 1])),
    ];
    let expected = [
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(0, 10.0, 10.0),
        },
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(1, 50.0, 10.0),
        },
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(2, 90.0, 10.0),
        },
        DeviceEvent::TouchCancel(on_screen(2)),
        DeviceEvent::TouchCancel(on_screen(0)),
        DeviceEvent::TouchCancel(on_screen(1)),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn each_pointer_of_a_gesture_has_the_kind_of_its_tool() {
    let both = vec![finger(0, 12.0, 10.0), pen(1, 48.0, 10.0)];
    let recorded = vec![
        touch(SCREEN, TouchAction::Down(finger(0, 10.0, 10.0))),
        touch(SCREEN, TouchAction::Down(pen(1, 50.0, 10.0))),
        touch(SCREEN, TouchAction::Move(both)),
    ];
    let expected = [
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(0, 10.0, 10.0),
        },
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: sample(SCREEN, 1, PointerKind::Pen, at(50.0, 10.0)),
        },
        DeviceEvent::TouchMotion(finger_at(0, 12.0, 10.0)),
        DeviceEvent::TouchMotion(sample(SCREEN, 1, PointerKind::Pen, at(48.0, 10.0))),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn two_devices_keep_apart_their_pointers_of_the_same_id() {
    let recorded = vec![
        touch(SCREEN, TouchAction::Down(finger(0, 10.0, 10.0))),
        touch(TABLET, TouchAction::Down(pen(0, 50.0, 50.0))),
        touch(SCREEN, TouchAction::Up(finger(0, 10.0, 10.0))),
    ];
    let expected = [
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: finger_at(0, 10.0, 10.0),
        },
        DeviceEvent::TouchDown {
            surface: SURFACE,
            sample: sample(TABLET, 0, PointerKind::Pen, at(50.0, 50.0)),
        },
        DeviceEvent::TouchUp(finger_at(0, 10.0, 10.0)),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn a_mouse_dragged_outside_the_surface_leaves_it_after_the_release() {
    let recorded = vec![
        mouse(MouseAction::Move(hovering(5.0, 5.0))),
        mouse(MouseAction::Move(hovering(10.0, 10.0))),
        mouse(press(holding(10.0, 10.0), Button::Primary)),
        mouse(MouseAction::Move(holding(150.0, 20.0))),
        mouse(release(hovering(150.0, 20.0), Button::Primary)),
        mouse(MouseAction::Move(hovering(30.0, 30.0))),
    ];
    let expected = [
        enter(5.0, 5.0),
        motion(10.0, 10.0),
        button(Button::Primary, PressState::Pressed, at(10.0, 10.0), 3),
        motion(150.0, 20.0),
        button(Button::Primary, PressState::Released, at(150.0, 20.0), 5),
        leave(),
        enter(30.0, 30.0),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn a_mouse_click_inside_the_surface_neither_leaves_nor_enters_it_again() {
    let recorded = vec![
        mouse(MouseAction::Move(hovering(10.0, 10.0))),
        mouse(press(holding(10.0, 10.0), Button::Secondary)),
        mouse(release(hovering(10.0, 10.0), Button::Secondary)),
        mouse(press(holding(10.0, 10.0), Button::Other)),
        mouse(release(hovering(10.0, 10.0), Button::Other)),
        mouse(MouseAction::Move(hovering(12.0, 10.0))),
        mouse(MouseAction::Exit),
    ];
    let expected = [
        enter(10.0, 10.0),
        button(Button::Secondary, PressState::Pressed, at(10.0, 10.0), 2),
        button(Button::Secondary, PressState::Released, at(10.0, 10.0), 3),
        button(Button::Other, PressState::Pressed, at(10.0, 10.0), 4),
        button(Button::Other, PressState::Released, at(10.0, 10.0), 5),
        motion(12.0, 10.0),
        leave(),
    ];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn a_mouse_cancel_leaves_the_surface_and_keeps_the_mouse() {
    let recorded = vec![
        mouse(MouseAction::Move(hovering(10.0, 10.0))),
        mouse(MouseAction::Cancel),
        mouse(MouseAction::Move(hovering(20.0, 20.0))),
    ];
    let expected = [enter(10.0, 10.0), leave(), enter(20.0, 20.0)];
    assert_eq!(translate(recorded, Density::ONE), expected);
}

#[test]
fn the_first_mouse_event_after_the_surface_is_shown_enters_it() {
    let mut translator = MotionTranslator::default();
    let moved = mouse(MouseAction::Move(hovering(10.0, 10.0)));
    assert_eq!(translator.translate(moved, None), []);
    let pressed = mouse(press(holding(10.0, 10.0), Button::Primary));
    let expected = [
        enter(10.0, 10.0),
        button(Button::Primary, PressState::Pressed, at(10.0, 10.0), 2),
    ];
    assert_eq!(
        translator.translate(pressed, Some(shown(Density::ONE))),
        expected
    );
}
