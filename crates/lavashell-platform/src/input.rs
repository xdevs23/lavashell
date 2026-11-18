// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{InputModifiers, TouchId};
use lavashell_core::view::{Cursor, TouchSample};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PointerDevice(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyboardDevice(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputSerial(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerPress {
    pub pointer: PointerDevice,
    pub serial: InputSerial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Pressed,
    Repeated,
    Released,
}

impl KeyAction {
    #[must_use]
    pub fn press_state(self) -> PressState {
        match self {
            Self::Pressed | Self::Repeated => PressState::Pressed,
            Self::Released => PressState::Released,
        }
    }
}

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
    Axis {
        pointer: PointerDevice,
        at: Point,
        delta: Point,
    },
    Key {
        keyboard: KeyboardDevice,
        code: u32,
        stroke: KeyStroke,
        action: KeyAction,
    },
    Modifiers {
        keyboard: KeyboardDevice,
        modifiers: InputModifiers,
    },
    KeyboardFocus {
        keyboard: KeyboardDevice,
        surface: Option<SurfaceId>,
    },
    PointerRemoved(PointerDevice),
    KeyboardRemoved(KeyboardDevice),
    TouchDown {
        surface: SurfaceId,
        sample: TouchSample,
    },
    TouchMotion(TouchSample),
    TouchUp(TouchSample),
    TouchCancel(TouchId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceEventKey {
    PointerPosition(PointerDevice),
    Scroll(PointerDevice),
    TouchPosition(TouchId),
}

impl Merge for DeviceEvent {
    type Key = DeviceEventKey;

    fn effect(&self) -> MergeEffect<DeviceEventKey> {
        match self {
            Self::PointerMotion { pointer, .. } => MergeEffect::OverwritesAndFences {
                key: DeviceEventKey::PointerPosition(*pointer),
                fenced: DeviceEventKey::Scroll(*pointer),
            },
            Self::Axis { pointer, .. } => MergeEffect::OverwritesAndFences {
                key: DeviceEventKey::Scroll(*pointer),
                fenced: DeviceEventKey::PointerPosition(*pointer),
            },
            Self::TouchMotion(sample) => {
                MergeEffect::Overwrites(DeviceEventKey::TouchPosition(sample.id))
            }
            _ => MergeEffect::Appends,
        }
    }

    fn absorb(&mut self, later: Self) {
        match (self, later) {
            (
                Self::Axis { at, delta, .. },
                Self::Axis {
                    at: later_at,
                    delta: later_delta,
                    ..
                },
            ) => {
                *at = later_at;
                *delta = delta.offset(later_delta.x, later_delta.y);
            }
            (earlier, later) => *earlier = later,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputRequest {
    SetCursor(PointerDevice, Cursor),
}

impl Merge for InputRequest {
    type Key = PointerDevice;

    fn effect(&self) -> MergeEffect<PointerDevice> {
        match *self {
            Self::SetCursor(pointer, _) => MergeEffect::Overwrites(pointer),
        }
    }
}
