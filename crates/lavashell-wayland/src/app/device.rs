// SPDX-License-Identifier: GPL-3.0-only
use super::{Point, PointerEventKind, Proxy, SurfaceId, sctk, wl_keyboard, wl_pointer};
use lavashell_platform::{Button, DeviceEvent, KeyboardDevice, PointerDevice, PressState};
use sctk::seat::pointer::{BTN_LEFT, BTN_RIGHT};

/// The protocol id of `proxy`, which is unique among the live objects on the connection.
fn protocol_id(proxy: &impl Proxy) -> u64 {
    u64::from(proxy.id().protocol_id())
}

pub(super) fn pointer_device(pointer: &wl_pointer::WlPointer) -> PointerDevice {
    PointerDevice(protocol_id(pointer))
}

pub(super) fn keyboard_device(keyboard: &wl_keyboard::WlKeyboard) -> KeyboardDevice {
    KeyboardDevice(protocol_id(keyboard))
}

/// The event reported by `kind` for `pointer` at `at` on `surface`.
pub(super) fn pointer_event(
    pointer: PointerDevice,
    surface: SurfaceId,
    at: Point,
    kind: &PointerEventKind,
) -> DeviceEvent {
    match *kind {
        PointerEventKind::Enter { .. } => DeviceEvent::PointerEnter {
            pointer,
            surface,
            at,
        },
        PointerEventKind::Leave { .. } => DeviceEvent::PointerLeave { pointer },
        PointerEventKind::Motion { .. } => DeviceEvent::PointerMotion { pointer, at },
        PointerEventKind::Press { button, .. } => {
            button_event(pointer, button, PressState::Pressed, at)
        }
        PointerEventKind::Release { button, .. } => {
            button_event(pointer, button, PressState::Released, at)
        }
        PointerEventKind::Axis {
            horizontal,
            vertical,
            ..
        } => DeviceEvent::Axis {
            pointer,
            at,
            delta: crate::logical::point(horizontal.absolute, vertical.absolute),
        },
    }
}

/// The event for the button with the evdev `code` on `pointer`.
fn button_event(pointer: PointerDevice, code: u32, state: PressState, at: Point) -> DeviceEvent {
    let button = match code {
        BTN_LEFT => Button::Primary,
        BTN_RIGHT => Button::Secondary,
        _ => Button::Other,
    };
    DeviceEvent::Button {
        pointer,
        button,
        state,
        at,
    }
}

#[cfg(test)]
mod tests;
