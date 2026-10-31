// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, View, wl_pointer, wl_seat, wl_surface};
use crate::clipboard::{DragEvent, DragGesture, DragRequest, DropEvent};
use crate::{Cancellation, Error};
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::DropAcceptance;

pub(super) type DragObserver = Box<dyn FnMut(DragEvent, &mut View) -> Option<DropAcceptance>>;
pub(super) type DropObserver = Box<dyn FnMut(DropEvent, &mut View) -> bool>;
pub(super) type DragSourceObserver = Box<dyn FnMut(DragGesture, &mut View) -> Option<DragRequest>>;

impl Platform {
    pub(crate) fn complete_drag_request(
        &mut self,
        origin: &wl_surface::WlSurface,
        done: crate::clipboard::DragCompletion,
        result: Result<lavashell_core::transfer::DragAction, Error>,
    ) {
        let id = self.find_surface(origin);
        let result = match id {
            Some(id) if self.surface_records.is_open(id) => result,
            _ => Err(Cancellation::SurfaceClosed.into()),
        };
        let Some(view) = id.and_then(|id| self.surface_records.view_mut(id)) else {
            log::warn!(
                "a drag ended after its source surface was removed, so its result is not reported"
            );
            return;
        };
        done(result, view);
    }

    pub(crate) fn dispatch_drag_event(
        &mut self,
        event: DragEvent,
        device: u32,
    ) -> Option<DropAcceptance> {
        let view = self.surface_records.open_view_mut(event.surface())?;
        if !matches!(&event, DragEvent::Enter(_) | DragEvent::Motion(_)) {
            if let Some(adapter) = &mut self.config.text_transfers
                && adapter.owns(&event)
            {
                return adapter.drag(&event, device, view);
            }
            return self
                .config
                .drag_observer
                .as_mut()
                .and_then(|callback| callback(event, view));
        }
        let accepted = self
            .config
            .drag_observer
            .as_mut()
            .and_then(|callback| callback(event.clone(), view));
        if let Some(adapter) = &mut self.config.text_transfers {
            if accepted.is_some() {
                adapter.application_accepted(&event);
            } else {
                return adapter.drag(&event, device, view);
            }
        }
        accepted
    }

    /// Reports to the view a drag that left the surface, was dropped on it or was cancelled over it. An
    /// acceptance returned for such a drag has no effect and is only logged.
    pub(crate) fn end_drag_event(&mut self, event: DragEvent, device: u32) {
        if let Some(acceptance) = self.dispatch_drag_event(event, device) {
            log::warn!(
                "the acceptance of {} came for a drag that already ended, so it is ignored",
                acceptance.mime_type
            );
        }
    }

    pub(crate) fn dispatch_drop_event(&mut self, mut event: DropEvent) -> bool {
        if !self.surface_records.is_open(event.surface) {
            event.data = Err(Cancellation::SurfaceClosed.into());
        }
        let Some(view) = self.surface_records.view_mut(event.surface) else {
            return false;
        };
        if let Some(adapter) = &mut self.config.text_transfers
            && adapter.has_pending(event.id)
        {
            return adapter.receive(event, view);
        }
        self.config
            .drop_observer
            .as_mut()
            .is_some_and(|callback| callback(event, view))
    }

    pub(crate) fn dispatch_drag_source(
        &mut self,
        gesture: DragGesture,
        device: u32,
    ) -> Option<DragRequest> {
        let view = self.surface_records.open_view_mut(gesture.surface)?;
        let fallback = DragGesture {
            surface: gesture.surface,
            origin: gesture.origin,
            position: gesture.position,
        };
        self.config
            .drag_source_observer
            .as_mut()
            .and_then(|callback| callback(gesture, view))
            .or_else(|| {
                self.config
                    .text_transfers
                    .as_ref()
                    .and_then(|adapter| adapter.source(&fallback, device, view))
            })
    }

    pub(crate) fn has_surface(&self, surface: SurfaceId) -> bool {
        self.surfaces.contains_key(&surface)
    }

    pub(crate) fn clipboard_view(&mut self, surface: SurfaceId) -> Option<&mut View> {
        self.surface_records.open_view_mut(surface)
    }

    pub(crate) fn clipboard_surface_for(
        &self,
        surface: SurfaceId,
    ) -> Option<wl_surface::WlSurface> {
        if !self.surface_records.is_open(surface) {
            return None;
        }
        self.surfaces.get(&surface).map(|native| native.wl.clone())
    }

    pub(crate) fn clipboard_view_for(
        &mut self,
        surface: &wl_surface::WlSurface,
    ) -> Option<&mut View> {
        let id = self.find_surface(surface)?;
        self.surface_records.open_view_mut(id)
    }

    pub(crate) fn clipboard_input_device(
        &self,
        surface: SurfaceId,
    ) -> Result<wl_seat::WlSeat, Error> {
        let mut devices = self
            .devices
            .iter()
            .filter(|device| device.focus == Some(surface));
        let first = devices.next().ok_or(Error::NoFocusedInputDevice)?;
        if devices.next().is_some() {
            return Err(Error::SeveralFocusedInputDevices);
        }
        Ok(first.seat.clone())
    }

    /// The id of the open surface `surface`.
    pub(crate) fn transfer_surface(&self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        (self.find_surface(surface)).filter(|&id| self.surface_records.is_open(id))
    }

    pub(crate) fn transfer_surface_live(&self, surface: &wl_surface::WlSurface) -> bool {
        self.transfer_surface(surface).is_some()
    }

    pub(crate) fn transfer_pointer_device(
        &self,
        pointer: &wl_pointer::WlPointer,
    ) -> Option<wl_seat::WlSeat> {
        self.devices
            .iter()
            .find(|device| {
                device
                    .pointer
                    .as_ref()
                    .is_some_and(|current| current.pointer() == pointer)
            })
            .map(|device| device.seat.clone())
    }
}
