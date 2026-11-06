// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    Button, DeviceEvent, InputSerial, KeyboardDevice, PointerDevice, PressState,
};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{wl_keyboard, wl_pointer};
use sctk::seat::pointer::{BTN_LEFT, BTN_RIGHT, PointerEventKind};
use smithay_client_toolkit as sctk;

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

fn input_serial(serial: u32) -> InputSerial {
    InputSerial(u64::from(serial))
}

pub(super) fn protocol_serial(serial: InputSerial) -> Result<u32, Error> {
    u32::try_from(serial.0).map_err(Error::ProtocolSerial)
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
        PointerEventKind::Press { button, serial, .. } => {
            button_event(pointer, button, PressState::Pressed, at, serial)
        }
        PointerEventKind::Release { button, serial, .. } => {
            button_event(pointer, button, PressState::Released, at, serial)
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

/// The event for the button with the evdev `code` on `pointer`, reported with `serial`.
fn button_event(
    pointer: PointerDevice,
    code: u32,
    state: PressState,
    at: Point,
    serial: u32,
) -> DeviceEvent {
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
        serial: input_serial(serial),
    }
}

#[cfg(test)]
mod tests;
