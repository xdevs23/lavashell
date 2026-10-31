// SPDX-License-Identifier: GPL-3.0-only
use super::{DragGesture, DragRequest, native_action};
use crate::{
    Cancellation, Error,
    app::Platform,
    clipboard::{
        sources::{Drag, DragProgress, Payload, Source},
        state::PointerGrab,
    },
};
use lavashell_core::{geometry::Point, transfer::DragAction};
use sctk::reexports::client::{
    Proxy,
    protocol::{wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface},
};
use smithay_client_toolkit as sctk;

impl Platform {
    pub(crate) fn transfer_pointer_press(
        &mut self,
        pointer: &WlPointer,
        surface: &WlSurface,
        serial: u32,
        button: u32,
        at: Point,
    ) {
        let Some(seat) = self.transfer_pointer_device(pointer) else {
            return;
        };
        if let Some(device) = self
            .clipboard
            .devices
            .iter_mut()
            .find(|device| device.seat == seat)
            && device.grab.is_none()
        {
            device.grab = Some(PointerGrab {
                surface: surface.clone(),
                serial,
                button,
                origin: at,
            });
        }
    }

    pub(crate) fn transfer_pointer_release(
        &mut self,
        pointer: &WlPointer,
        surface: &WlSurface,
        button: u32,
    ) {
        let Some(seat) = self.transfer_pointer_device(pointer) else {
            return;
        };
        if let Some(device) = self
            .clipboard
            .devices
            .iter_mut()
            .find(|device| device.seat == seat)
            && device
                .grab
                .as_ref()
                .is_some_and(|grab| grab.button == button && grab.surface == *surface)
        {
            device.grab = None;
        }
    }

    pub(crate) fn transfer_pointer_cancel(&mut self, seat: &WlSeat) {
        if let Some(device) = self
            .clipboard
            .devices
            .iter_mut()
            .find(|device| &device.seat == seat)
        {
            device.grab = None;
        }
        self.cancel_drag_sources(seat);
    }

    pub(crate) fn transfer_pointer_motion(&mut self, pointer: &WlPointer, at: Point) -> bool {
        let Some(seat) = self.transfer_pointer_device(pointer) else {
            return false;
        };
        let Some(device) = (self.clipboard.devices.iter_mut()).find(|device| device.seat == seat)
        else {
            return false;
        };
        let Some(grab) = device.grab.take_if(|grab| grab.button == 0x110) else {
            return false;
        };
        let device = device.device.inner().id().protocol_id();
        let gesture = (self.transfer_surface(&grab.surface)).map(|surface| DragGesture {
            surface,
            origin: grab.origin,
            position: at,
        });
        let Some(request) = gesture.and_then(|gesture| self.dispatch_drag_source(gesture, device))
        else {
            if let Some(device) =
                (self.clipboard.devices.iter_mut()).find(|device| device.seat == seat)
            {
                device.grab = Some(grab);
            }
            return false;
        };
        self.start_drag_request(seat, grab, request);
        true
    }

    fn start_drag_request(&mut self, seat: WlSeat, grab: PointerGrab, request: DragRequest) {
        let result = (|| {
            let manager = (self.clipboard.manager.as_ref()).ok_or(Error::DragUnavailable)?;
            if manager.data_device_manager().version() < 3 {
                return Err(Error::DragCompletionUnavailable);
            }
            if request.actions.is_empty() {
                return Err(Error::DragWithoutAction);
            }
            if !self.transfer_surface_live(&grab.surface) {
                return Err(Cancellation::SurfaceClosed.into());
            }
            let device = (self.clipboard.devices.iter())
                .find(|device| device.seat == seat)
                .ok_or(Cancellation::InputDeviceGone)?;
            let actions = [DragAction::Copy, DragAction::Move]
                .into_iter()
                .filter(|action| request.actions.contains(*action))
                .fold(
                    sctk::reexports::client::protocol::wl_data_device_manager::DndAction::empty(),
                    |bits, action| bits | native_action(action),
                );
            let source =
                manager.create_drag_and_drop_source(&self.qh, request.data.formats(), actions);
            source.start_drag(&device.device, &grab.surface, None, grab.serial);
            Ok(source)
        })();
        match result {
            Ok(source) => {
                if let Some(started) = request.started {
                    match self.clipboard_view_for(&grab.surface) {
                        Some(view) => started(view),
                        None => log::warn!("a drag started after its source surface closed"),
                    }
                }
                self.clipboard.payloads.push(Payload {
                    source: Source::Drag(source),
                    data: request.data,
                    seat,
                    drag: Some(Drag {
                        surface: grab.surface,
                        progress: DragProgress {
                            pending: 0,
                            sent: 0,
                            failure: None,
                            finished: false,
                            action: None,
                            done: request.done,
                            received: request.received,
                        },
                    }),
                });
            }
            Err(error) => self.complete_drag_request(&grab.surface, request.done, Err(error)),
        }
    }
}
