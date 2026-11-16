// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::DragSourceError;
use crate::clipboard::{into_nonblocking, portable_action};
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use lavashell_core::transfer::Cancellation;
use lavashell_platform::{
    ClipboardEvent, ClipboardRequest, DragSourceEvent, DragSourceId, DragSourceRequest, FormatId,
    SelectionId,
};
use sctk::data_device_manager::{WritePipe, data_source::DataSourceHandler};
use sctk::reexports::client::protocol::{
    wl_data_device_manager::DndAction, wl_data_source::WlDataSource, wl_seat::WlSeat,
};
use sctk::reexports::client::{Connection, QueueHandle};

impl Translator {
    pub(super) fn execute_clipboard(&mut self, request: ClipboardRequest) {
        let event = match request {
            ClipboardRequest::SetSelection(id, formats) => ClipboardEvent::SelectionSet(
                id,
                self.clipboard.offer_selection(&self.qh, id, formats),
            ),
            ClipboardRequest::Receive(id, format) => {
                ClipboardEvent::ReceiveStarted(id, self.clipboard.receive_selection(format))
            }
        };
        self.links.report(event);
    }

    pub(super) fn execute_drag_source(
        &mut self,
        request: DragSourceRequest,
    ) -> Result<(), DragSourceError> {
        match request {
            DragSourceRequest::Start {
                drag,
                pointer,
                surface,
                formats,
                actions,
            } => {
                if self.clipboard.has_drag(drag) {
                    return Err(DragSourceError::DuplicateDrag(drag));
                }
                let origin = self.live_wl_surface(surface);
                let seat = self.devices.pointer_seat(pointer).cloned();
                let started = self
                    .clipboard
                    .start_drag(&self.qh, drag, origin, seat, formats, actions);
                self.links.report(DragSourceEvent::Started(drag, started));
            }
            DragSourceRequest::Cancel(drag) => self.clipboard.end_drag(drag),
        }
        Ok(())
    }

    pub(super) fn note_input_serial(&mut self, seat: WlSeat, serial: ProtocolSerial) {
        if let Some(event) = self.clipboard.note_serial(seat, serial) {
            self.links.report(event);
        }
    }

    pub(super) fn cancel_drag_sources(&mut self, seat: &WlSeat) {
        for event in self.clipboard.cancel_drags_of(seat) {
            self.links.report(event);
        }
    }

    pub(super) fn remove_transfer_seat(&mut self, seat: &WlSeat) {
        for event in self.clipboard.remove_seat(seat) {
            self.links.report(event);
        }
    }

    fn send_selection(&self, id: SelectionId, format: String, pipe: WritePipe) {
        let send = ClipboardEvent::SourceSend(id, FormatId(format), into_nonblocking(pipe));
        self.links.report(send);
    }

    fn send_drag(&self, drag: DragSourceId, format: String, pipe: WritePipe) {
        let local = self.clipboard.local_offer_of_drag(drag, &pipe);
        self.links.report(DragSourceEvent::Send {
            drag,
            format: FormatId(format),
            pipe: into_nonblocking(pipe),
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
        if let Some(id) = self.clipboard.selection_id(source) {
            self.send_selection(id, mime, pipe);
        } else if let Some(drag) = self.clipboard.source_drag(source) {
            self.send_drag(drag, mime, pipe);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(id) = self.clipboard.selection_id(source) {
            self.clipboard.cancel_selection(id);
            self.links.report(ClipboardEvent::SourceCancelled(id));
        } else if let Some(drag) = self.clipboard.source_drag(source) {
            self.clipboard.end_drag(drag);
            let reason = Cancellation::SourceCancelled;
            self.links.report(DragSourceEvent::Cancelled(drag, reason));
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(drag) = self.clipboard.source_drag(source) {
            self.clipboard.end_drag(drag);
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
        if let Some(drag) = self.clipboard.source_drag(source) {
            let action = portable_action(action);
            self.links
                .report(DragSourceEvent::ActionSelected(drag, action));
        }
    }
}
