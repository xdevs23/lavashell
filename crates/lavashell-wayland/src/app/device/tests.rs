// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::units::DpExt;
use sctk::seat::pointer::{AxisScroll, BTN_MIDDLE};

const MOUSE: PointerDevice = PointerDevice(1);
const WINDOW: SurfaceId = SurfaceId(0);

fn at() -> Point {
    Point::new(4.dp(), 2.dp())
}

fn pressed(code: u32) -> DeviceEvent {
    let kind = PointerEventKind::Press {
        time: 0,
        button: code,
        serial: 0,
    };
    pointer_event(MOUSE, WINDOW, at(), &kind)
}

fn pressed_button(button: Button) -> DeviceEvent {
    DeviceEvent::Button {
        pointer: MOUSE,
        button,
        state: PressState::Pressed,
        at: at(),
    }
}

#[test]
fn the_left_button_is_primary_the_right_one_secondary_and_every_other_button_other() {
    assert_eq!(pressed(BTN_LEFT), pressed_button(Button::Primary));
    assert_eq!(pressed(BTN_RIGHT), pressed_button(Button::Secondary));
    assert_eq!(pressed(BTN_MIDDLE), pressed_button(Button::Other));
}

#[test]
fn a_scroll_reports_the_absolute_distance_along_each_axis_as_its_delta() {
    let scroll = |absolute| AxisScroll {
        absolute,
        ..AxisScroll::default()
    };
    let kind = PointerEventKind::Axis {
        time: 0,
        horizontal: scroll(3.0),
        vertical: scroll(-5.0),
        source: None,
    };
    let expected = DeviceEvent::Axis {
        pointer: MOUSE,
        at: at(),
        delta: Point::new(3.dp(), (-5).dp()),
    };
    assert_eq!(pointer_event(MOUSE, WINDOW, at(), &kind), expected);
}
