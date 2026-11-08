// SPDX-License-Identifier: GPL-3.0-only
use super::{pipes::nonblocking, state::SelectionSource};
use crate::translator::Translator;
use lavashell_platform::{
    ClipboardEvent, ClipboardRequest, FormatId, ReceiveRefusal, SelectionId, SelectionRefusal,
};
use sctk::data_device_manager::{
    WritePipe,
    data_offer::{SelectionOffer, receive},
};
use sctk::reexports::client::protocol::{wl_data_source::WlDataSource, wl_seat::WlSeat};
use smithay_client_toolkit as sctk;
use std::io;
use std::os::fd::OwnedFd;

impl Translator {
    /// Executes `request` of the runtime and reports its outcome.
    pub(crate) fn execute_clipboard(&mut self, request: ClipboardRequest) {
        let event = match request {
            ClipboardRequest::SetSelection(id, formats) => {
                ClipboardEvent::SelectionSet(id, self.offer_selection(id, formats))
            }
            ClipboardRequest::Receive(id, format) => {
                ClipboardEvent::ReceiveStarted(id, self.receive_selection(format))
            }
        };
        self.links.report(event);
    }

    /// Notes the serial of an input event of `seat`. When the input comes from another seat than the last input, the
    /// runtime gets the selection offered to that seat.
    pub(crate) fn note_input_serial(&mut self, seat: WlSeat, serial: u32) {
        let other_seat = (self.clipboard.serial.as_ref()).is_none_or(|(last, _)| *last != seat);
        self.clipboard.serial = Some((seat, serial));
        if other_seat {
            self.report_selection();
        }
    }

    /// Reports the formats of the selection offered to the seat of the last input event.
    pub(crate) fn report_selection(&self) {
        let formats = self.selection_offer().map_or_else(Vec::new, |offer| {
            offer.with_mime_types(|types| types.iter().cloned().map(FormatId).collect())
        });
        self.links.report(ClipboardEvent::SelectionOffered(formats));
    }

    /// The id of the selection set through `source`, or `None` for a drag source.
    pub(super) fn selection_id(&self, source: &WlDataSource) -> Option<SelectionId> {
        (self.clipboard.selections.iter())
            .find(|selection| selection.source.inner() == source)
            .map(|selection| selection.id)
    }

    /// Hands the pipe of a client that pastes the selection `id` in `format` to the runtime, which writes the
    /// selection to it, or the failure to make the pipe nonblocking.
    pub(super) fn send_selection(&self, id: SelectionId, format: String, pipe: WritePipe) {
        let pipe = nonblocking(&pipe)
            .map(|()| pipe.into())
            .map_err(io::Error::from);
        let send = ClipboardEvent::SourceSend(id, FormatId(format), pipe);
        self.links.report(send);
    }

    /// Destroys the source of the selection `id`, which the compositor cancelled.
    pub(super) fn cancel_selection(&mut self, id: SelectionId) {
        self.clipboard
            .selections
            .retain(|selection| selection.id != id);
        self.links.report(ClipboardEvent::SourceCancelled(id));
    }

    fn offer_selection(
        &mut self,
        id: SelectionId,
        formats: Vec<FormatId>,
    ) -> Result<(), SelectionRefusal> {
        let manager = (self.clipboard.manager.as_ref()).ok_or(SelectionRefusal::Unavailable)?;
        let (seat, serial) = (self.clipboard.serial.clone()).ok_or(SelectionRefusal::NoInput)?;
        let device = (self.clipboard.devices.iter())
            .find(|device| device.seat == seat)
            .ok_or(SelectionRefusal::InputDeviceGone)?;
        let formats = formats.into_iter().map(|format| format.0);
        let source = manager.create_copy_paste_source(&self.qh, formats);
        source.set_selection(&device.device, serial);
        (self.clipboard.selections).push(SelectionSource { id, source, seat });
        Ok(())
    }

    /// Asks the client that set the selection to write it in `format` to a new pipe, and returns the nonblocking read
    /// end of that pipe.
    fn receive_selection(&self, format: FormatId) -> Result<OwnedFd, ReceiveRefusal> {
        let offer = self.selection_offer().ok_or(ReceiveRefusal::Empty)?;
        if !offer.with_mime_types(|types| types.contains(&format.0)) {
            return Err(ReceiveRefusal::NoFormat);
        }
        let pipe = receive(offer.inner(), format.0).map_err(ReceiveRefusal::Pipe)?;
        nonblocking(&pipe).map_err(|errno| ReceiveRefusal::Pipe(errno.into()))?;
        Ok(pipe.into())
    }

    /// The selection offered to the seat of the last input event.
    fn selection_offer(&self) -> Option<SelectionOffer> {
        let (seat, _) = self.clipboard.serial.as_ref()?;
        (self.clipboard.devices.iter())
            .find(|device| &device.seat == seat)?
            .device
            .data()
            .selection_offer()
    }
}
