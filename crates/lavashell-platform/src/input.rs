// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{InputModifiers, TouchId};
use lavashell_core::view::{Cursor, TouchSample};

/// Identifies a pointer connected to the host while it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PointerDevice(pub u64);

/// Identifies a keyboard connected to the host while it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyboardDevice(pub u64);

/// The number given by the host to an input event. A request made on behalf of that input contains it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputSerial(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Primary,
    /// The button that opens a context menu.
    Secondary,
    /// Any other button. It takes the surface over without reaching the view.
    Other,
}

/// Whether a pointer button or a key went down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressState {
    Pressed,
    Released,
}

/// What a pointer, keyboard or touch screen connected to the host did. A position is in dp, relative to the surface
/// under the pointer or to the surface on which the touch point went down.
#[derive(Debug, PartialEq)]
pub enum DeviceEvent {
    PointerEnter {
        pointer: PointerDevice,
        surface: SurfaceId,
        at: Point,
    },
    PointerLeave {
        pointer: PointerDevice,
    },
    PointerMotion {
        pointer: PointerDevice,
        at: Point,
    },
    Button {
        pointer: PointerDevice,
        button: Button,
        state: PressState,
        at: Point,
        serial: InputSerial,
    },
    /// A scroll wheel or touchpad scrolled by `delta` along each axis. A positive component of `delta` scrolls toward
    /// the right or the bottom of the content.
    Axis {
        pointer: PointerDevice,
        at: Point,
        delta: Point,
    },
    Key {
        keyboard: KeyboardDevice,
        stroke: KeyStroke,
        state: PressState,
    },
    Modifiers {
        keyboard: KeyboardDevice,
        modifiers: InputModifiers,
    },
    /// The keyboard focus moved to `surface`, or left every surface.
    KeyboardFocus {
        keyboard: KeyboardDevice,
        surface: Option<SurfaceId>,
    },
    PointerRemoved(PointerDevice),
    KeyboardRemoved(KeyboardDevice),
    /// A touch point went down on `surface`.
    TouchDown {
        surface: SurfaceId,
        sample: TouchSample,
    },
    TouchMotion(TouchSample),
    TouchUp(TouchSample),
    /// A touch point ended without going up.
    TouchCancel(TouchId),
}

/// What the runtime asks a platform to do with a pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputRequest {
    /// Shows the cursor for the pointer and replaces the cursor set by an earlier request.
    SetCursor(PointerDevice, Cursor),
}
