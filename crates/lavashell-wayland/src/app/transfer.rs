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
        let Some(surface) = (self.surfaces.iter_mut()).find(|surface| &surface.wl == origin) else {
            log::warn!(
                "a drag ended after its source surface was removed, so its result is not reported"
            );
            return;
        };
        let result = if surface.lifecycle.is_open() {
            result
        } else {
            Err(Cancellation::SurfaceClosed.into())
        };
        done(result, &mut surface.view);
    }

    pub(crate) fn dispatch_drag_event(
        &mut self,
        event: DragEvent,
        device: u32,
    ) -> Option<DropAcceptance> {
        let surface = self.surfaces.get_mut(event.surface())?;
        if !surface.lifecycle.is_open() {
            return None;
        }
        if !matches!(&event, DragEvent::Enter(_) | DragEvent::Motion(_)) {
            if let Some(adapter) = &mut self.config.text_transfers
                && adapter.owns(&event)
            {
                return adapter.drag(&event, device, &surface.view);
            }
            return self
                .config
                .drag_observer
                .as_mut()
                .and_then(|callback| callback(event, &mut surface.view));
        }
        let accepted = self
            .config
            .drag_observer
            .as_mut()
            .and_then(|callback| callback(event.clone(), &mut surface.view));
        if let Some(adapter) = &mut self.config.text_transfers {
            if accepted.is_some() {
                adapter.application_accepted(&event);
            } else {
                return adapter.drag(&event, device, &surface.view);
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
        let Some(surface) = self.surfaces.get_mut(event.surface) else {
            return false;
        };
        if !surface.lifecycle.is_open() {
            event.data = Err(Cancellation::SurfaceClosed.into());
        }
        if let Some(adapter) = &mut self.config.text_transfers
            && adapter.has_pending(event.id)
        {
            return adapter.receive(event, &mut surface.view);
        }
        self.config
            .drop_observer
            .as_mut()
            .is_some_and(|callback| callback(event, &mut surface.view))
    }

    pub(crate) fn dispatch_drag_source(
        &mut self,
        gesture: DragGesture,
        device: u32,
    ) -> Option<DragRequest> {
        let surface = self.surfaces.get_mut(gesture.surface)?;
        if !surface.lifecycle.is_open() {
            return None;
        }
        let fallback = DragGesture {
            surface: gesture.surface,
            origin: gesture.origin,
            position: gesture.position,
        };
        self.config
            .drag_source_observer
            .as_mut()
            .and_then(|callback| callback(gesture, &mut surface.view))
            .or_else(|| {
                self.config
                    .text_transfers
                    .as_ref()
                    .and_then(|adapter| adapter.source(&fallback, device, &surface.view))
            })
    }

    pub(crate) fn surface_index(&self, id: SurfaceId) -> Option<usize> {
        self.surfaces.iter().position(|surface| surface.id == id)
    }

    pub(crate) fn surface_id(&self, index: usize) -> Option<SurfaceId> {
        self.surfaces.get(index).map(|surface| surface.id)
    }

    pub(crate) fn clipboard_view(&mut self, index: usize) -> Option<&mut View> {
        self.surfaces
            .get_mut(index)
            .filter(|surface| surface.lifecycle.is_open())
            .map(|surface| &mut surface.view)
    }

    pub(crate) fn clipboard_surface_for(&self, index: usize) -> Option<wl_surface::WlSurface> {
        self.surfaces
            .get(index)
            .filter(|surface| surface.lifecycle.is_open())
            .map(|surface| surface.wl.clone())
    }

    pub(crate) fn clipboard_view_for(
        &mut self,
        surface: &wl_surface::WlSurface,
    ) -> Option<&mut View> {
        self.surfaces
            .iter_mut()
            .find(|current| &current.wl == surface && current.lifecycle.is_open())
            .map(|surface| &mut surface.view)
    }

    pub(crate) fn clipboard_input_device(&self, surface: usize) -> Result<wl_seat::WlSeat, Error> {
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

    pub(crate) fn transfer_surface_index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfaces
            .iter()
            .position(|current| &current.wl == surface && current.lifecycle.is_open())
    }

    pub(crate) fn transfer_surface_live(&self, surface: &wl_surface::WlSurface) -> bool {
        self.transfer_surface_index(surface).is_some()
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
