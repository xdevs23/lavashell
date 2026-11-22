// SPDX-License-Identifier: GPL-3.0-only
use super::device::{keyboard_device, pointer_device};
use crate::sctk;
use lavashell_core::geometry::Point;
use lavashell_core::ui::TouchId;
use lavashell_core::view::TouchSample;
use lavashell_platform::{DeviceEvent, InputTime, KeyboardDevice, PointerDevice, TouchMove};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{wl_keyboard, wl_seat, wl_touch};
use sctk::seat::pointer::ThemedPointer;
use std::collections::BTreeMap;

pub(crate) enum SeatDevice {
    Pointer(ThemedPointer),
    Keyboard(wl_keyboard::WlKeyboard),
    Touch(wl_touch::WlTouch),
}

struct SeatDevices {
    seat: wl_seat::WlSeat,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    touch: Option<wl_touch::WlTouch>,
    touch_positions: BTreeMap<i32, Point>,
}

impl SeatDevices {
    fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            pointer: None,
            keyboard: None,
            touch: None,
            touch_positions: BTreeMap::new(),
        }
    }

    fn pointer_device(&self) -> Option<PointerDevice> {
        (self.pointer.as_ref()).map(|themed| pointer_device(themed.pointer()))
    }

    fn keyboard_device(&self) -> Option<KeyboardDevice> {
        self.keyboard.as_ref().map(keyboard_device)
    }

    fn cancel_touches(&mut self) -> Vec<DeviceEvent> {
        (std::mem::take(&mut self.touch_positions).into_keys())
            .map(|point| DeviceEvent::TouchCancel(touch_id(&self.seat, point)))
            .collect()
    }
}

pub(crate) struct SeatSet(Vec<SeatDevices>);

impl SeatSet {
    pub(crate) fn new(seats: impl Iterator<Item = wl_seat::WlSeat>) -> Self {
        Self(seats.map(SeatDevices::new).collect())
    }

    pub(crate) fn insert(&mut self, seat: wl_seat::WlSeat) -> bool {
        let new = !self.contains(&seat);
        if new {
            self.0.push(SeatDevices::new(seat));
        }
        new
    }

    pub(crate) fn contains(&self, seat: &wl_seat::WlSeat) -> bool {
        self.0.iter().any(|known| &known.seat == seat)
    }

    pub(crate) fn attach(&mut self, seat: &wl_seat::WlSeat, device: SeatDevice) {
        let Some(devices) = self.devices_mut(seat) else {
            return;
        };
        match device {
            SeatDevice::Pointer(pointer) => devices.pointer = Some(pointer),
            SeatDevice::Keyboard(keyboard) => devices.keyboard = Some(keyboard),
            SeatDevice::Touch(touch) => devices.touch = Some(touch),
        }
    }

    pub(crate) fn detach_pointer(&mut self, seat: &wl_seat::WlSeat) -> Option<DeviceEvent> {
        let themed = self.devices_mut(seat)?.pointer.take()?;
        Some(DeviceEvent::PointerRemoved(pointer_device(
            themed.pointer(),
        )))
    }

    pub(crate) fn detach_keyboard(&mut self, seat: &wl_seat::WlSeat) -> Option<DeviceEvent> {
        let keyboard = self.devices_mut(seat)?.keyboard.take()?;
        let removed = DeviceEvent::KeyboardRemoved(keyboard_device(&keyboard));
        keyboard.release();
        Some(removed)
    }

    pub(crate) fn detach_touch(&mut self, seat: &wl_seat::WlSeat) -> Vec<DeviceEvent> {
        let cancelled = self.cancel_seat_touches(seat);
        if let Some(touch) = self
            .devices_mut(seat)
            .and_then(|devices| devices.touch.take())
        {
            touch.release();
        }
        cancelled
    }

    pub(crate) fn remove(&mut self, seat: &wl_seat::WlSeat) -> Vec<DeviceEvent> {
        let Some(index) = self.0.iter().position(|known| &known.seat == seat) else {
            return Vec::new();
        };
        let removed = self.0.remove(index);
        let pointer = removed.pointer_device().map(DeviceEvent::PointerRemoved);
        let keyboard = removed.keyboard_device().map(DeviceEvent::KeyboardRemoved);
        pointer.into_iter().chain(keyboard).collect()
    }

    pub(crate) fn cancel_seat_touches(&mut self, seat: &wl_seat::WlSeat) -> Vec<DeviceEvent> {
        (self.devices_mut(seat)).map_or_else(Vec::new, SeatDevices::cancel_touches)
    }

    pub(crate) fn pointer_seat(&self, pointer: PointerDevice) -> Option<&wl_seat::WlSeat> {
        (self.0.iter())
            .find(|seat| seat.pointer_device() == Some(pointer))
            .map(|seat| &seat.seat)
    }

    pub(crate) fn keyboard_seat(
        &self,
        keyboard: &wl_keyboard::WlKeyboard,
    ) -> Option<&wl_seat::WlSeat> {
        (self.0.iter())
            .find(|seat| seat.keyboard.as_ref() == Some(keyboard))
            .map(|seat| &seat.seat)
    }

    pub(crate) fn themed_pointer(&self, pointer: PointerDevice) -> Option<&ThemedPointer> {
        (self.0.iter())
            .filter_map(|seat| seat.pointer.as_ref())
            .find(|themed| pointer_device(themed.pointer()) == pointer)
    }

    pub(crate) fn touch_down(
        &mut self,
        touch: &wl_touch::WlTouch,
        point: i32,
        at: Point,
    ) -> Option<(wl_seat::WlSeat, TouchSample)> {
        let devices = self.touch_devices(touch)?;
        if devices.touch_positions.insert(point, at).is_some() {
            log::debug!("touch point {point} went down again before it went up");
        }
        let sample = TouchSample::finger(touch_id(&devices.seat, point), at);
        Some((devices.seat.clone(), sample))
    }

    pub(crate) fn touch_up(
        &mut self,
        touch: &wl_touch::WlTouch,
        point: i32,
    ) -> Option<DeviceEvent> {
        let devices = self.touch_devices(touch)?;
        let at = devices.touch_positions.remove(&point)?;
        let sample = TouchSample::finger(touch_id(&devices.seat, point), at);
        Some(DeviceEvent::TouchUp(sample))
    }

    pub(crate) fn touch_motion(
        &mut self,
        touch: &wl_touch::WlTouch,
        point: i32,
        at: Point,
        time: InputTime,
    ) -> Option<DeviceEvent> {
        let devices = self.touch_devices(touch)?;
        *devices.touch_positions.get_mut(&point)? = at;
        let sample = TouchSample::finger(touch_id(&devices.seat, point), at);
        Some(DeviceEvent::TouchMotion {
            earlier: Vec::new(),
            latest: TouchMove { sample, time },
        })
    }

    pub(crate) fn cancel_touches(&mut self, touch: &wl_touch::WlTouch) -> Vec<DeviceEvent> {
        (self.touch_devices(touch)).map_or_else(Vec::new, SeatDevices::cancel_touches)
    }

    fn devices_mut(&mut self, seat: &wl_seat::WlSeat) -> Option<&mut SeatDevices> {
        (self.0.iter_mut()).find(|known| &known.seat == seat)
    }

    fn touch_devices(&mut self, touch: &wl_touch::WlTouch) -> Option<&mut SeatDevices> {
        (self.0.iter_mut()).find(|seat| seat.touch.as_ref() == Some(touch))
    }
}

fn touch_id(seat: &wl_seat::WlSeat, point: i32) -> TouchId {
    TouchId {
        seat: i64::from(seat.id().protocol_id()),
        point,
    }
}
