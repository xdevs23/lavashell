// SPDX-License-Identifier: GPL-3.0-only
use crate::host::AospReports;
use lavashell_core::geometry::{Point, Rect, ScaledViewport};
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::units::Px;
use lavashell_core::view::TouchSample;
use lavashell_platform::{
    Button, DeviceEvent, InputSerial, InputTime, PointerDevice, PressState, TouchMove,
};

pub(crate) const MOUSE: PointerDevice = PointerDevice(0);

#[derive(Clone, Debug, PartialEq)]
pub struct MotionEvent(pub(crate) Motion);

impl MotionEvent {
    #[must_use]
    pub fn touch(seat: i64, action: TouchAction) -> Self {
        Self(Motion::Touch { seat, action })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Motion {
    Touch { seat: i64, action: TouchAction },
    Mouse(MouseAction),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TouchAction {
    Down(TouchPointer),
    Up(TouchPointer),
    Move(Vec<TouchPositions>),
    Cancel(Vec<i32>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TouchPositions {
    pub time: InputTime,
    pub pointers: Vec<TouchPointer>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPointer {
    pub id: i32,
    pub kind: PointerKind,
    pub x: Px,
    pub y: Px,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MouseAction {
    Move(MousePosition),
    Button {
        position: MousePosition,
        button: Button,
        state: PressState,
    },
    Exit,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MousePosition {
    pub(crate) x: Px,
    pub(crate) y: Px,
    pub(crate) held: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShownSurface {
    pub(crate) id: SurfaceId,
    pub(crate) scaled_viewport: ScaledViewport,
}

impl ShownSurface {
    fn point(self, x: Px, y: Px) -> Point {
        let ScaledViewport { viewport, scale } = self.scaled_viewport;
        scale.point(viewport, x, y)
    }

    fn contains_mouse_at(self, position: MousePosition) -> bool {
        let area = Rect::from_size(self.scaled_viewport.viewport.size);
        position.held || area.contains(self.point(position.x, position.y))
    }
}

#[derive(Default)]
pub(crate) struct MotionTranslator {
    serials: u64,
    entered: Option<SurfaceId>,
}

impl MotionTranslator {
    pub(crate) fn report(
        &mut self,
        event: MotionEvent,
        shown: Option<ShownSurface>,
        reports: &AospReports,
    ) {
        for device_event in self.translate(event, shown) {
            reports.report(device_event);
        }
    }

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

    fn mouse_events(&mut self, action: MouseAction, shown: ShownSurface) -> Vec<DeviceEvent> {
        match action {
            MouseAction::Move(position) if shown.contains_mouse_at(position) => {
                let at = shown.point(position.x, position.y);
                let motion = DeviceEvent::PointerMotion { pointer: MOUSE, at };
                vec![self.enter(at, shown).unwrap_or(motion)]
            }
            MouseAction::Button {
                position,
                button,
                state,
            } => {
                let at = shown.point(position.x, position.y);
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

    fn enter(&mut self, at: Point, shown: ShownSurface) -> Option<DeviceEvent> {
        let entered = self.entered.replace(shown.id);
        (entered != Some(shown.id)).then_some(DeviceEvent::PointerEnter {
            pointer: MOUSE,
            surface: shown.id,
            at,
        })
    }

    fn leave(&mut self) -> Option<DeviceEvent> {
        let entered = self.entered.take();
        entered.map(|_| DeviceEvent::PointerLeave { pointer: MOUSE })
    }
}

fn touch_events(seat: i64, action: TouchAction, shown: ShownSurface) -> Vec<DeviceEvent> {
    let id = |point| TouchId { seat, point };
    let sample = |pointer: TouchPointer| TouchSample {
        id: id(pointer.id),
        at: shown.point(pointer.x, pointer.y),
        kind: pointer.kind,
    };
    match action {
        TouchAction::Down(pointer) => vec![DeviceEvent::TouchDown {
            surface: shown.id,
            sample: sample(pointer),
        }],
        TouchAction::Up(pointer) => vec![DeviceEvent::TouchUp(sample(pointer))],
        TouchAction::Move(batched) => (batched.into_iter())
            .flat_map(|TouchPositions { time, pointers }| {
                pointers
                    .into_iter()
                    .map(move |pointer| DeviceEvent::TouchMotion {
                        earlier: Vec::new(),
                        latest: TouchMove {
                            sample: sample(pointer),
                            time,
                        },
                    })
            })
            .collect(),
        TouchAction::Cancel(points) => (points.into_iter())
            .map(|point| DeviceEvent::TouchCancel(id(point)))
            .collect(),
    }
}
