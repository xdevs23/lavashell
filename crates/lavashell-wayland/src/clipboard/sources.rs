// SPDX-License-Identifier: GPL-3.0-only
use super::drag::{drag_device, native_actions, portable_action};
use super::pipes::{PipeIdentity, nonblocking};
use super::state::SourceDrag;
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{Cancellation, DragActions, TransferId};
use lavashell_platform::{
    DragDevice, DragRefusal, DragSourceEvent, DragSourceId, DragSourceRequest, FormatId,
    PointerDevice,
};
use sctk::data_device_manager::{WritePipe, data_source::DataSourceHandler};
use sctk::reexports::client::protocol::{
    wl_data_device_manager::DndAction, wl_data_source::WlDataSource, wl_seat::WlSeat,
};
use sctk::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit as sctk;
use std::io;
use std::os::fd::AsFd;

impl Translator {
    /// Executes `request` of the runtime about a drag started by the app.
    pub(crate) fn execute_drag_source(&mut self, request: DragSourceRequest) {
        match request {
            DragSourceRequest::Start {
                drag,
                pointer,
                surface,
                formats,
                actions,
            } => {
                let started = self.start_drag(drag, pointer, surface, formats, actions);
                self.links.report(DragSourceEvent::Started(drag, started));
            }
            DragSourceRequest::Cancel(drag) => {
                let _cancelled = self.clipboard.drags.remove(&drag);
            }
        }
    }

    /// Starts the drag `drag` from `surface` with the pointer `pointer`, and returns the device of the seat of that
    /// pointer.
    fn start_drag(
        &mut self,
        drag: DragSourceId,
        pointer: PointerDevice,
        surface: SurfaceId,
        formats: Vec<FormatId>,
        actions: DragActions,
    ) -> Result<DragDevice, DragRefusal> {
        let manager = (self.clipboard.manager.as_ref()).ok_or(DragRefusal::Unavailable)?;
        if manager.data_device_manager().version() < 3 {
            return Err(DragRefusal::CompletionUnavailable);
        }
        let origin = self
            .open_wl_surface(surface)
            .ok_or(DragRefusal::SurfaceClosed)?;
        let seat = self
            .pointer_seat(pointer)
            .cloned()
            .ok_or(DragRefusal::InputDeviceGone)?;
        let transfers = (self.clipboard.devices.iter())
            .find(|device| device.seat == seat)
            .ok_or(DragRefusal::InputDeviceGone)?;
        let serial = transfers.primary_press.ok_or(DragRefusal::NoPress)?;
        let formats = formats.into_iter().map(|format| format.0);
        let actions = native_actions(actions);
        let source = manager.create_drag_and_drop_source(&self.qh, formats, actions);
        source.start_drag(&transfers.device, &origin, None, serial);
        let device = drag_device(transfers.device.inner());
        let _no_drag_had_the_id = (self.clipboard.drags).insert(drag, SourceDrag { seat, source });
        Ok(device)
    }

    /// Cancels each drag started by the runtime with a pointer of `seat`, whose pointer disappeared.
    pub(crate) fn cancel_drag_sources(&mut self, seat: &WlSeat) {
        let cancelled: Vec<_> = (self.clipboard.drags)
            .extract_if(.., |_, drag| &drag.seat == seat)
            .map(|(id, _)| id)
            .collect();
        for id in cancelled {
            let gone = Cancellation::InputDeviceGone;
            self.links.report(DragSourceEvent::Cancelled(id, gone));
        }
    }

    fn source_drag(&self, source: &WlDataSource) -> Option<DragSourceId> {
        (self.clipboard.drags.iter())
            .find(|(_, drag)| drag.source.inner() == source)
            .map(|(id, _)| *id)
    }

    /// The drag offered to the app on `seat` whose data arrives through `pipe`, when exactly one such drag exists. A
    /// drag from the app to itself sends its data through that pipe.
    fn local_offer(&self, pipe: &impl AsFd, seat: &WlSeat) -> Option<TransferId> {
        let identity = PipeIdentity::read(pipe)?;
        let mut matches = (self.clipboard.offers.iter())
            .filter(|(_, offer)| offer.pipe == Some(identity) && &offer.seat == seat)
            .map(|(id, _)| *id);
        let id = matches.next()?;
        matches.next().is_none().then_some(id)
    }

    /// Hands the pipe of a client that receives the drag `drag` in `format` to the runtime, which writes the dragged
    /// data to it, or the failure to make the pipe nonblocking.
    fn send_drag(&mut self, drag: DragSourceId, format: String, pipe: WritePipe) {
        let local = (self.clipboard.drags.get(&drag))
            .and_then(|known| self.local_offer(&pipe, &known.seat));
        let pipe = nonblocking(&pipe)
            .map(|()| pipe.into())
            .map_err(io::Error::from);
        self.links.report(DragSourceEvent::Send {
            drag,
            format: FormatId(format),
            pipe,
            local,
        });
    }
}

impl DataSourceHandler for Translator {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        if let Some(id) = self.selection_id(source) {
            self.send_selection(id, mime, pipe);
        } else if let Some(drag) = self.source_drag(source) {
            self.send_drag(drag, mime, pipe);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(id) = self.selection_id(source) {
            self.cancel_selection(id);
        } else if let Some(drag) = self.source_drag(source) {
            let _cancelled = self.clipboard.drags.remove(&drag);
            let reason = Cancellation::SourceCancelled;
            self.links.report(DragSourceEvent::Cancelled(drag, reason));
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(drag) = self.source_drag(source) {
            let _finished = self.clipboard.drags.remove(&drag);
            self.links.report(DragSourceEvent::Finished(drag));
        }
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        action: DndAction,
    ) {
        if let Some(drag) = self.source_drag(source) {
            let action = portable_action(action);
            self.links
                .report(DragSourceEvent::ActionSelected(drag, action));
        }
    }
}
