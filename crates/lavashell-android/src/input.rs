// SPDX-License-Identifier: GPL-3.0-only
//! Motion events on the Surface shown by an AOSP host, and the device events of the platform that they are translated
//! into.

use crate::host::layout::Layout;
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::units::Px;
use lavashell_core::view::TouchSample;
use lavashell_platform::{Button, DeviceEvent, InputSerial, PointerDevice, PressState, Reports};

/// Every mouse moves the one cursor that AOSP draws on a display.
pub(crate) const MOUSE: PointerDevice = PointerDevice(0);

/// A motion event on the Surface shown by the host. `motion_event_of` reads it from an android.view.MotionEvent.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionEvent(pub(crate) Motion);

/// What a touch gesture or the mouse did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Motion {
    /// `seat` is the device id of the event. A pointer id is unique only within one input device.
    Touch {
        seat: u32,
        action: TouchAction,
    },
    Mouse(MouseAction),
}

/// What the pointers of a touch gesture did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TouchAction {
    Down(TouchPointer),
    Up(TouchPointer),
    /// Contains every pointer that is down.
    Move(Vec<TouchPointer>),
    /// Contains the ids of the pointers that ended without going up.
    Cancel(Vec<i32>),
}

/// A pointer of a touch gesture at its position in pixels of the Surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TouchPointer {
    /// The id given by the host to the pointer while it touches the Surface.
    pub(crate) id: i32,
    pub(crate) kind: PointerKind,
    pub(crate) x: Px,
    pub(crate) y: Px,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MouseAction {
    Move(MousePosition),
    Button {
        position: MousePosition,
        button: Button,
        state: PressState,
    },
    /// The mouse left the view of the host with every button up.
    Exit,
    /// AOSP took the gesture of the mouse away.
    Cancel,
}

/// The position of the mouse in pixels of the Surface after an action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MousePosition {
    pub(crate) x: Px,
    pub(crate) y: Px,
    /// Whether a button is down.
    pub(crate) held: bool,
}

/// The surface of the runtime shown in the Surface of the host, and its layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShownSurface {
    pub(crate) id: SurfaceId,
    pub(crate) layout: Layout,
}

impl ShownSurface {
    /// Whether the mouse at `position` is over the surface. A mouse with a button down stays over it.
    fn contains_mouse_at(self, position: MousePosition) -> bool {
        let at = self.layout.point(position.x, position.y);
        position.held || self.layout.contains(at)
    }
}

/// Translates the motion events of the host into device events on the shown surface, and numbers the motion events.
#[derive(Default)]
pub(crate) struct MotionTranslator {
    /// The number of motion events received, which is the serial of the newest one.
    serials: u64,
    /// The shown surface that the mouse is over.
    entered: Option<SurfaceId>,
}

impl MotionTranslator {
    /// Reports the device events of `event` on `shown`.
    pub(crate) fn report(
        &mut self,
        event: MotionEvent,
        shown: Option<ShownSurface>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) {
        for device_event in self.translate(event, shown) {
            reports.report(device_event);
        }
    }

    /// The device events of `event` on `shown`. While no surface is shown, a motion event produces no device events.
    pub(crate) fn translate(
        &mut self,
        MotionEvent(motion): MotionEvent,
        shown: Option<ShownSurface>,
    ) -> Vec<DeviceEvent> {
        self.serials += 1;
        let Some(shown) = shown else {
            return Vec::new();
        };
        match motion {
            Motion::Touch { seat, action } => touch_events(seat, action, shown),
            Motion::Mouse(action) => self.mouse_events(action, shown),
        }
    }

    /// The pointer events of `action` on `shown`. The mouse enters `shown` at its first position over it. It leaves
    /// when it exits, when AOSP cancels its gesture, and when it is outside `shown` with every button up.
    fn mouse_events(&mut self, action: MouseAction, shown: ShownSurface) -> Vec<DeviceEvent> {
        match action {
            MouseAction::Move(position) if shown.contains_mouse_at(position) => {
                let at = shown.layout.point(position.x, position.y);
                let motion = DeviceEvent::PointerMotion { pointer: MOUSE, at };
                vec![self.enter(at, shown).unwrap_or(motion)]
            }
            MouseAction::Button {
                position,
                button,
                state,
            } => {
                let at = shown.layout.point(position.x, position.y);
                let press = DeviceEvent::Button {
                    pointer: MOUSE,
                    button,
                    state,
                    at,
                    serial: InputSerial(self.serials),
                };
                let entered = self.enter(at, shown);
                let left = if shown.contains_mouse_at(position) {
                    None
                } else {
                    self.leave()
                };
                entered.into_iter().chain([press]).chain(left).collect()
            }
            MouseAction::Move(_) | MouseAction::Exit | MouseAction::Cancel => {
                self.leave().into_iter().collect()
            }
        }
    }

    /// Enters `shown` at `at`, unless the mouse is over it.
    fn enter(&mut self, at: Point, shown: ShownSurface) -> Option<DeviceEvent> {
        let entered = self.entered.replace(shown.id);
        (entered != Some(shown.id)).then_some(DeviceEvent::PointerEnter {
            pointer: MOUSE,
            surface: shown.id,
            at,
        })
    }

    /// Leaves the surface that the mouse is over.
    fn leave(&mut self) -> Option<DeviceEvent> {
        let entered = self.entered.take();
        entered.map(|_| DeviceEvent::PointerLeave { pointer: MOUSE })
    }
}

/// The touch events of `action` on `shown`, made by the pointers of the input device `seat`.
fn touch_events(seat: u32, action: TouchAction, shown: ShownSurface) -> Vec<DeviceEvent> {
    let id = |point| TouchId { seat, point };
    let sample = |pointer: TouchPointer| TouchSample {
        id: id(pointer.id),
        at: shown.layout.point(pointer.x, pointer.y),
        kind: pointer.kind,
    };
    match action {
        TouchAction::Down(pointer) => vec![DeviceEvent::TouchDown {
            surface: shown.id,
            sample: sample(pointer),
        }],
        TouchAction::Up(pointer) => vec![DeviceEvent::TouchUp(sample(pointer))],
        TouchAction::Move(pointers) => (pointers.into_iter())
            .map(|pointer| DeviceEvent::TouchMotion(sample(pointer)))
            .collect(),
        TouchAction::Cancel(points) => (points.into_iter())
            .map(|point| DeviceEvent::TouchCancel(id(point)))
            .collect(),
    }
}
