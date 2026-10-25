// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Connection, InputEvent, Platform, Point, PointerKind, QueueHandle, SeatDevices, TouchId,
    TouchSample, sctk, wl_seat, wl_surface, wl_touch,
};
use sctk::seat::touch::TouchHandler;
use std::collections::BTreeMap;

pub(super) struct TouchPoints {
    /// The seat number contained in the touch ids passed to the views, which distinguishes touches on different
    /// seats.
    seat: u32,
    points: BTreeMap<i32, Down>,
}

/// A touch point that is down: the surface on which it went down and its last reported position. The position is
/// stored here because the `up` event of the protocol has no position.
struct Down {
    surface: usize,
    at: Point,
}

impl TouchPoints {
    pub(super) fn new(seat: u32) -> Self {
        Self {
            seat,
            points: BTreeMap::new(),
        }
    }

    fn id(&self, point: i32) -> TouchId {
        TouchId {
            seat: self.seat,
            point,
        }
    }

    fn sample(&self, point: i32, at: Point) -> TouchSample {
        TouchSample {
            id: self.id(point),
            at,
            kind: PointerKind::Finger,
        }
    }

    fn down(&mut self, point: i32, surface: usize, at: Point) -> (usize, InputEvent) {
        if self.points.insert(point, Down { surface, at }).is_some() {
            log::debug!("touch point {point} went down again before it went up");
        }
        (surface, InputEvent::TouchDown(self.sample(point, at)))
    }

    fn motion(&mut self, point: i32, at: Point) -> Option<(usize, InputEvent)> {
        let down = self.points.get_mut(&point)?;
        down.at = at;
        let surface = down.surface;
        Some((surface, InputEvent::TouchMove(self.sample(point, at))))
    }

    fn up(&mut self, point: i32) -> Option<(usize, InputEvent)> {
        let down = self.points.remove(&point)?;
        Some((
            down.surface,
            InputEvent::TouchUp(self.sample(point, down.at)),
        ))
    }

    /// Removes every touch point and returns a cancel event for each, together with the surface that receives the
    /// event.
    pub(super) fn cancel(&mut self) -> Vec<(usize, InputEvent)> {
        let points = std::mem::take(&mut self.points);
        (points.into_iter())
            .map(|(point, down)| (down.surface, InputEvent::TouchCancel(self.id(point))))
            .collect()
    }
}

impl Platform {
    fn touch_seat(&mut self, touch: &wl_touch::WlTouch) -> Option<&mut SeatDevices> {
        (self.devices.iter_mut()).find(|seat| seat.touch.as_ref() == Some(touch))
    }

    fn touch_input(&mut self, input: Option<(usize, InputEvent)>) {
        if let Some((surface, input)) = input {
            self.surfaces[surface].view.input(input);
        }
    }

    /// Sends a cancel event for every touch point of `seat` to its surface. Touch points of other seats remain
    /// down.
    pub(super) fn cancel_touches(&mut self, seat: &wl_seat::WlSeat) {
        let cancels = (self.devices.iter_mut())
            .find(|device| &device.seat == seat)
            .map(|device| device.touches.cancel())
            .unwrap_or_default();
        for cancel in cancels {
            self.touch_input(Some(cancel));
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
        let Some(surface) = self.index(&surface) else {
            return;
        };
        let at = crate::logical::point(position.0, position.1);
        let Some(seat) = self.touch_seat(touch) else {
            return;
        };
        let input = seat.touches.down(id, surface, at);
        let seat = seat.seat.clone();
        self.clipboard.note_serial(seat, serial);
        self.touch_input(Some(input));
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
        let input = (self.touch_seat(touch)).and_then(|seat| seat.touches.up(id));
        self.touch_input(input);
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
        let input = (self.touch_seat(touch)).and_then(|seat| seat.touches.motion(id, at));
        self.touch_input(input);
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
        if let Some(seat) = self.touch_seat(touch).map(|seat| seat.seat.clone()) {
            self.cancel_touches(&seat);
        }
    }
}

#[cfg(test)]
mod tests;
