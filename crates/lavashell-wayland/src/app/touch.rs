// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, SeatDevices};
use lavashell_core::geometry::Point;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::view::TouchSample;
use lavashell_platform::DeviceEvent;
use sctk::reexports::client::protocol::{wl_seat, wl_surface, wl_touch};
use sctk::reexports::client::{Connection, Proxy, QueueHandle};
use sctk::seat::touch::TouchHandler;
use smithay_client_toolkit as sctk;

fn touch_id(seat: &wl_seat::WlSeat, point: i32) -> TouchId {
    TouchId {
        seat: seat.id().protocol_id(),
        point,
    }
}

fn finger_sample(id: TouchId, at: Point) -> TouchSample {
    TouchSample {
        id,
        at,
        kind: PointerKind::Finger,
    }
}

impl Platform {
    fn touch_devices(&mut self, touch: &wl_touch::WlTouch) -> Option<&mut SeatDevices> {
        (self.devices.iter_mut()).find(|seat| seat.touch.as_ref() == Some(touch))
    }

    /// Reports that every touch point on the touch screen of `seat` ended without going up.
    pub(super) fn cancel_touches(&mut self, seat: &wl_seat::WlSeat) {
        let positions = (self.seat_devices(seat))
            .map(|devices| std::mem::take(&mut devices.touch_positions))
            .unwrap_or_default();
        for point in positions.into_keys() {
            self.route_input(DeviceEvent::TouchCancel(touch_id(seat, point)));
        }
    }
}

impl TouchHandler for Platform {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        serial: u32,
        _: u32,
        surface: wl_surface::WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(surface) = self.find_surface(&surface) else {
            return;
        };
        let Some(devices) = self.touch_devices(touch) else {
            return;
        };
        let at = crate::logical::point(position.0, position.1);
        if devices.touch_positions.insert(id, at).is_some() {
            log::debug!("touch point {id} went down again before it went up");
        }
        let seat = devices.seat.clone();
        let sample = finger_sample(touch_id(&seat, id), at);
        self.clipboard.note_serial(seat, serial);
        self.route_input(DeviceEvent::TouchDown { surface, sample });
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        _: u32,
        _: u32,
        id: i32,
    ) {
        if let Some(devices) = self.touch_devices(touch)
            && let Some(at) = devices.touch_positions.remove(&id)
        {
            let sample = finger_sample(touch_id(&devices.seat, id), at);
            self.route_input(DeviceEvent::TouchUp(sample));
        }
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        _: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let at = crate::logical::point(position.0, position.1);
        if let Some(devices) = self.touch_devices(touch)
            && let Some(last) = devices.touch_positions.get_mut(&id)
        {
            *last = at;
            let sample = finger_sample(touch_id(&devices.seat, id), at);
            self.route_input(DeviceEvent::TouchMotion(sample));
        }
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
    }

    fn orientation(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
    ) {
    }

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, touch: &wl_touch::WlTouch) {
        let seat = (self.touch_devices(touch)).map(|devices| devices.seat.clone());
        if let Some(seat) = seat {
            self.cancel_touches(&seat);
        }
    }
}
