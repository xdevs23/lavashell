// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::view::InputTime;
use lavashell_platform::DeviceEvent;
use sctk::reexports::client::protocol::{wl_surface, wl_touch};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::seat::touch::TouchHandler;
use std::time::Duration;

impl TouchHandler for Translator {
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
        let at = crate::logical::convert_to_point(position.0, position.1);
        if let Some((seat, sample)) = self.devices.touch_down(touch, id, at) {
            self.note_input_serial(seat, ProtocolSerial::new(serial));
            self.links
                .report(DeviceEvent::TouchDown { surface, sample });
        }
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
        if let Some(up) = self.devices.touch_up(touch, id) {
            self.links.report(up);
        }
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        millis: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let at = crate::logical::convert_to_point(position.0, position.1);
        let time = InputTime(Duration::from_millis(u64::from(millis)));
        if let Some(motion) = self.devices.touch_motion(touch, id, at, time) {
            self.links.report(motion);
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
        for cancelled in self.devices.cancel_touches(touch) {
            self.links.report(cancelled);
        }
    }
}
