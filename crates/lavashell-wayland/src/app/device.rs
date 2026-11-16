// SPDX-License-Identifier: GPL-3.0-only
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    Button, DeviceEvent, InputSerial, KeyboardDevice, PointerDevice, PressState,
};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{wl_keyboard, wl_pointer};
use sctk::seat::pointer::{BTN_LEFT, BTN_RIGHT, PointerEventKind};

mod error;

pub use error::DeviceError;

pub(crate) fn protocol_id(proxy: &impl Proxy) -> u64 {
    u64::from(proxy.id().protocol_id())
}

pub(crate) fn pointer_device(pointer: &wl_pointer::WlPointer) -> PointerDevice {
    PointerDevice(protocol_id(pointer))
}

pub(crate) fn keyboard_device(keyboard: &wl_keyboard::WlKeyboard) -> KeyboardDevice {
    KeyboardDevice(protocol_id(keyboard))
}

fn input_serial(serial: u32) -> InputSerial {
    InputSerial(u64::from(serial))
}

pub(crate) fn protocol_serial(serial: InputSerial) -> Result<ProtocolSerial, DeviceError> {
    u32::try_from(serial.0)
        .map(ProtocolSerial::new)
        .map_err(DeviceError::ProtocolSerial)
}

pub(crate) fn pointer_event(
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
            delta: crate::logical::convert_to_point(horizontal.absolute, vertical.absolute),
        },
    }
}

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
