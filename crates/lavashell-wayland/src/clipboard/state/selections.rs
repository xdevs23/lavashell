// SPDX-License-Identifier: GPL-3.0-only
use super::{ClipboardState, SeatSerial, SelectionSource};
use crate::clipboard::pipes::into_nonblocking;
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use crate::translator::Translator;
use lavashell_platform::{ClipboardEvent, FormatId, ReceiveRefusal, SelectionId, SelectionRefusal};
use sctk::data_device_manager::data_offer::{SelectionOffer, receive};
use sctk::reexports::client::QueueHandle;
use sctk::reexports::client::protocol::{wl_data_source::WlDataSource, wl_seat::WlSeat};
use std::os::fd::OwnedFd;

impl ClipboardState {
    pub(crate) fn note_serial(
        &mut self,
        seat: WlSeat,
        serial: ProtocolSerial,
    ) -> Option<ClipboardEvent> {
        let other_seat = (self.serial.as_ref()).is_none_or(|last| last.seat != seat);
        self.serial = Some(SeatSerial { seat, serial });
        other_seat.then(|| self.selection_offered())
    }

    pub(crate) fn selection_offered(&self) -> ClipboardEvent {
        let formats = self.selection_offer().map_or_else(Vec::new, |offer| {
            offer.with_mime_types(|types| types.iter().cloned().map(FormatId).collect())
        });
        ClipboardEvent::SelectionOffered(formats)
    }

    pub(crate) fn selection_id(&self, source: &WlDataSource) -> Option<SelectionId> {
        (self.selections.iter())
            .find(|selection| selection.source.inner() == source)
            .map(|selection| selection.id)
    }

    pub(crate) fn cancel_selection(&mut self, id: SelectionId) {
        self.selections.retain(|selection| selection.id != id);
    }

    pub(crate) fn offer_selection(
        &mut self,
        qh: &QueueHandle<Translator>,
        id: SelectionId,
        formats: Vec<FormatId>,
    ) -> Result<(), SelectionRefusal> {
        let manager = (self.manager.as_ref()).ok_or(SelectionRefusal::Unavailable)?;
        let SeatSerial { seat, serial } = (self.serial.clone()).ok_or(SelectionRefusal::NoInput)?;
        let device = (self.transfers_of(&seat)).ok_or(SelectionRefusal::InputDeviceGone)?;
        let formats = formats.into_iter().map(|format| format.0);
        let source = manager.create_copy_paste_source(qh, formats);
        source.set_selection(&device.device, serial.get());
        self.selections.push(SelectionSource { id, source, seat });
        Ok(())
    }

    pub(crate) fn receive_selection(&self, format: FormatId) -> Result<OwnedFd, ReceiveRefusal> {
        let offer = self.selection_offer().ok_or(ReceiveRefusal::Empty)?;
        if !offer.with_mime_types(|types| types.contains(&format.0)) {
            return Err(ReceiveRefusal::NoFormat);
        }
        let pipe = receive(offer.inner(), format.0).map_err(ReceiveRefusal::Pipe)?;
        into_nonblocking(pipe).map_err(ReceiveRefusal::Pipe)
    }

    pub(super) fn destroy_selection_offers_of(&self, seat: &WlSeat) {
        if let Some(offer) =
            (self.transfers_of(seat)).and_then(|device| device.device.data().selection_offer())
        {
            offer.destroy();
        }
    }

    pub(super) fn cancel_selections_of(&mut self, seat: &WlSeat) -> Vec<ClipboardEvent> {
        (self.selections)
            .extract_if(.., |selection| &selection.seat == seat)
            .map(|selection| ClipboardEvent::SourceCancelled(selection.id))
            .collect()
    }

    fn selection_offer(&self) -> Option<SelectionOffer> {
        let seat = &self.serial.as_ref()?.seat;
        self.transfers_of(seat)?.device.data().selection_offer()
    }
}
