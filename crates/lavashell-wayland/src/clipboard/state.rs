// SPDX-License-Identifier: GPL-3.0-only
use super::pipes::PipeIdentity;
use crate::Error;
use crate::app::Platform;
use lavashell_core::transfer::TransferId;
use lavashell_platform::{ClipboardEvent, DragSourceId, DragTargetEvent, SelectionId};
use lavashell_runtime::{ClipboardTransfers, TextClipboard};
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::data_device_manager::data_source::{CopyPasteSource, DragSource};
use sctk::data_device_manager::{DataDeviceManagerState, data_device::DataDevice};
use sctk::reexports::client::{QueueHandle, globals::GlobalList, protocol::wl_seat::WlSeat};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;

pub(super) struct SeatTransfers {
    pub seat: WlSeat,
    pub device: DataDevice,
    /// The drag offered to the app that is over a surface, from its enter until its leave or drop.
    pub entered: Option<TransferId>,
    /// The serial of the last press of the primary pointer button, which a drag started by the app needs.
    pub primary_press: Option<u32>,
}

/// A selection set by the runtime, until the compositor cancels it.
pub(super) struct SelectionSource {
    pub id: SelectionId,
    pub source: CopyPasteSource,
    pub seat: WlSeat,
}

/// A drag offered to the app, from its enter until it leaves, its device disappears or the runtime releases it.
pub(super) struct Offer {
    pub seat: WlSeat,
    pub native: DragOffer,
    /// The pipe through which the dropped data arrives, once the runtime asked for the data.
    pub pipe: Option<PipeIdentity>,
}

/// A drag started by the runtime, until it finishes or is cancelled.
pub(super) struct SourceDrag {
    pub seat: WlSeat,
    pub source: DragSource,
}

pub(crate) struct ClipboardState {
    pub(super) manager: Option<DataDeviceManagerState>,
    pub(super) devices: Vec<SeatTransfers>,
    /// The seat and the serial of the last input event. The clipboard of that seat is the one used by the runtime.
    pub(super) serial: Option<(WlSeat, u32)>,
    pub(super) selections: Vec<SelectionSource>,
    pub(super) offers: BTreeMap<TransferId, Offer>,
    next_offer_id: u64,
    pub(super) drags: BTreeMap<DragSourceId, SourceDrag>,
    pub(crate) transfers: ClipboardTransfers,
    pub(crate) text: TextClipboard,
}

impl ClipboardState {
    pub fn new(
        globals: &GlobalList,
        qh: &QueueHandle<Platform>,
        seats: impl Iterator<Item = WlSeat>,
        transfers: ClipboardTransfers,
        text: TextClipboard,
    ) -> Self {
        let mut state = Self {
            manager: DataDeviceManagerState::bind(globals, qh).ok(),
            devices: Vec::new(),
            serial: None,
            selections: Vec::new(),
            offers: BTreeMap::new(),
            next_offer_id: 0,
            drags: BTreeMap::new(),
            transfers,
            text,
        };
        for seat in seats {
            state.add_seat(qh, seat);
        }
        state
    }

    pub fn add_seat(&mut self, qh: &QueueHandle<Platform>, seat: WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push(SeatTransfers {
                device: manager.get_data_device(qh, &seat),
                seat,
                entered: None,
                primary_press: None,
            });
        }
    }

    pub(super) fn new_offer_id(&mut self) -> Result<TransferId, Error> {
        let id = TransferId(self.next_offer_id);
        self.next_offer_id = id.0.checked_add(1).ok_or(Error::NumberDragOffer)?;
        Ok(id)
    }
}

impl Platform {
    pub(crate) fn shutdown_transfers(&mut self) {
        self.close_drags();
        self.clipboard.transfers.close();
        self.clipboard.selections.clear();
        self.clipboard.offers.clear();
        self.clipboard.drags.clear();
        self.clipboard.devices.clear();
    }

    pub(crate) fn remove_transfer_seat(&mut self, seat: &WlSeat) {
        let offers: Vec<_> = (self.clipboard.offers)
            .extract_if(.., |_, offer| &offer.seat == seat)
            .collect();
        for (id, offer) in offers {
            offer.native.destroy();
            self.drag_target_event(DragTargetEvent::Cancelled(id));
        }
        for device in self
            .clipboard
            .devices
            .iter()
            .filter(|device| &device.seat == seat)
        {
            if let Some(offer) = device.device.data().selection_offer() {
                offer.destroy();
            }
        }
        self.cancel_drag_sources(seat);
        let cancelled: Vec<_> = (self.clipboard.selections)
            .extract_if(.., |selection| &selection.seat == seat)
            .map(|selection| selection.id)
            .collect();
        for id in cancelled {
            (self.clipboard.transfers).handle(ClipboardEvent::SourceCancelled(id));
        }
        self.clipboard.devices.retain(|device| &device.seat != seat);
        if self
            .clipboard
            .serial
            .as_ref()
            .is_some_and(|(current, _)| current == seat)
        {
            self.clipboard.serial = None;
            self.report_selection();
        }
    }
}
