// SPDX-License-Identifier: GPL-3.0-only
use super::{ClipboardState, SourceDrag};
use crate::clipboard::drag::{drag_device, native_actions};
use crate::clipboard::pipes::PipeIdentity;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::transfer::{Cancellation, DragActions, TransferId};
use lavashell_platform::{DragDevice, DragRefusal, DragSourceEvent, DragSourceId, FormatId};
use sctk::reexports::client::protocol::{
    wl_data_source::{self, WlDataSource},
    wl_seat::WlSeat,
    wl_surface::WlSurface,
};
use sctk::reexports::client::{Proxy, QueueHandle};
use std::os::fd::AsFd;

impl ClipboardState {
    pub(crate) fn has_drag(&self, drag: DragSourceId) -> bool {
        self.drags.contains_key(&drag)
    }

    pub(crate) fn start_drag(
        &mut self,
        qh: &QueueHandle<Translator>,
        drag: DragSourceId,
        origin: Option<WlSurface>,
        seat: Option<WlSeat>,
        formats: Vec<FormatId>,
        actions: DragActions,
    ) -> Result<DragDevice, DragRefusal> {
        let manager = (self.manager.as_ref()).ok_or(DragRefusal::Unavailable)?;
        if manager.data_device_manager().version() < wl_data_source::EVT_DND_FINISHED_SINCE {
            return Err(DragRefusal::CompletionUnavailable);
        }
        let origin = origin.ok_or(DragRefusal::SurfaceClosed)?;
        let seat = seat.ok_or(DragRefusal::InputDeviceGone)?;
        let transfers = (self.transfers_of(&seat)).ok_or(DragRefusal::InputDeviceGone)?;
        let serial = transfers.primary_press.ok_or(DragRefusal::NoPress)?;
        let formats = formats.into_iter().map(|format| format.0);
        let actions = native_actions(actions);
        let source = manager.create_drag_and_drop_source(qh, formats, actions);
        source.start_drag(&transfers.device, &origin, None, serial.get());
        let device = drag_device(transfers.device.inner());
        let _vacant_after_the_duplicate_check =
            self.drags.insert(drag, SourceDrag { seat, source });
        Ok(device)
    }

    pub(crate) fn end_drag(&mut self, drag: DragSourceId) {
        let _ended = self.drags.remove(&drag);
    }

    pub(crate) fn source_drag(&self, source: &WlDataSource) -> Option<DragSourceId> {
        (self.drags.iter())
            .find(|(_, drag)| drag.source.inner() == source)
            .map(|(id, _)| *id)
    }

    pub(crate) fn local_offer_of_drag(
        &self,
        drag: DragSourceId,
        pipe: &impl AsFd,
    ) -> Option<TransferId> {
        let seat = &self.drags.get(&drag)?.seat;
        let identity = PipeIdentity::read(pipe)?;
        let mut matches = (self.offers.iter())
            .filter(|(_, offer)| offer.pipe == Some(identity) && &offer.seat == seat)
            .map(|(id, _)| *id);
        let id = matches.next()?;
        matches.next().is_none().then_some(id)
    }

    pub(crate) fn cancel_drags_of(&mut self, seat: &WlSeat) -> Vec<DragSourceEvent> {
        (self.drags)
            .extract_if(.., |_, drag| &drag.seat == seat)
            .map(|(id, _)| DragSourceEvent::Cancelled(id, Cancellation::InputDeviceGone))
            .collect()
    }
}
