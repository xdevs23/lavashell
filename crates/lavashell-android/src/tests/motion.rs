// SPDX-License-Identifier: GPL-3.0-only
//! Builders of the motion events that a host reports.

use crate::input::{Motion, MotionEvent, MouseAction, MousePosition, TouchAction, TouchPointer};
use lavashell_core::ui::PointerKind;
use lavashell_core::units::Px;
use lavashell_platform::{Button, PressState};

pub(super) fn finger(id: i32, x: f64, y: f64) -> TouchPointer {
    pointer(id, PointerKind::Finger, x, y)
}

pub(super) fn pen(id: i32, x: f64, y: f64) -> TouchPointer {
    pointer(id, PointerKind::Pen, x, y)
}

fn pointer(id: i32, kind: PointerKind, x: f64, y: f64) -> TouchPointer {
    TouchPointer {
        id,
        kind,
        x: Px::new(x),
        y: Px::new(y),
    }
}

/// A touch event made by the pointers of the input device `seat`.
pub(super) fn touch(seat: u32, action: TouchAction) -> MotionEvent {
    MotionEvent(Motion::Touch { seat, action })
}

pub(super) fn mouse(action: MouseAction) -> MotionEvent {
    MotionEvent(Motion::Mouse(action))
}

/// A position of the mouse with every button up.
pub(super) fn hovering(x: f64, y: f64) -> MousePosition {
    position(x, y, false)
}

/// A position of the mouse with a button down.
pub(super) fn holding(x: f64, y: f64) -> MousePosition {
    position(x, y, true)
}

fn position(x: f64, y: f64, held: bool) -> MousePosition {
    MousePosition {
        x: Px::new(x),
        y: Px::new(y),
        held,
    }
}

pub(super) fn press(position: MousePosition, button: Button) -> MouseAction {
    MouseAction::Button {
        position,
        button,
        state: PressState::Pressed,
    }
}

pub(super) fn release(position: MousePosition, button: Button) -> MouseAction {
    MouseAction::Button {
        position,
        button,
        state: PressState::Released,
    }
}
