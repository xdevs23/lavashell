// SPDX-License-Identifier: GPL-3.0-only
use super::pipes::PipeIdentity;
use crate::protocol_serial::ProtocolSerial;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::id_counter::IdCounter;
use lavashell_core::transfer::TransferId;
use lavashell_platform::{DragSourceId, PlatformEvent, SelectionId};
use sctk::data_device_manager::data_offer::DragOffer;
use sctk::data_device_manager::data_source::{CopyPasteSource, DragSource};
use sctk::data_device_manager::{DataDeviceManagerState, data_device::DataDevice};
use sctk::reexports::client::{QueueHandle, protocol::wl_seat::WlSeat};
use std::collections::BTreeMap;

mod drag_sources;
mod drag_targets;
mod selections;

struct SeatTransfers {
    seat: WlSeat,
    device: DataDevice,
    entered: Option<TransferId>,
    primary_press: Option<ProtocolSerial>,
}

#[derive(Clone)]
struct SeatSerial {
    seat: WlSeat,
    serial: ProtocolSerial,
}

struct SelectionSource {
    id: SelectionId,
    source: CopyPasteSource,
    seat: WlSeat,
}

struct DragTargetOffer {
    seat: WlSeat,
    native: DragOffer,
    pipe: Option<PipeIdentity>,
}

struct SourceDrag {
    seat: WlSeat,
    source: DragSource,
}

pub(crate) struct ClipboardState {
    manager: Option<DataDeviceManagerState>,
    devices: Vec<SeatTransfers>,
    serial: Option<SeatSerial>,
    selections: Vec<SelectionSource>,
    offers: BTreeMap<TransferId, DragTargetOffer>,
    offer_ids: IdCounter,
    drags: BTreeMap<DragSourceId, SourceDrag>,
}

impl ClipboardState {
    pub fn new(
        manager: Option<DataDeviceManagerState>,
        qh: &QueueHandle<Translator>,
        seats: impl Iterator<Item = WlSeat>,
    ) -> Self {
        let mut state = Self {
            manager,
            devices: Vec::new(),
            serial: None,
            selections: Vec::new(),
            offers: BTreeMap::new(),
            offer_ids: IdCounter::default(),
            drags: BTreeMap::new(),
        };
        for seat in seats {
            state.add_seat(qh, seat);
        }
        state
    }

    pub fn add_seat(&mut self, qh: &QueueHandle<Translator>, seat: WlSeat) {
        if let Some(manager) = &self.manager {
            self.devices.push(SeatTransfers {
                device: manager.get_data_device(qh, &seat),
                seat,
                entered: None,
                primary_press: None,
            });
        }
    }

    pub(crate) fn remove_seat(&mut self, seat: &WlSeat) -> Vec<PlatformEvent> {
        let mut events: Vec<PlatformEvent> = (self.cancel_offers_of(seat).into_iter())
            .map(PlatformEvent::from)
            .collect();
        self.destroy_selection_offers_of(seat);
        events.extend((self.cancel_drags_of(seat).into_iter()).map(PlatformEvent::from));
        events.extend((self.cancel_selections_of(seat).into_iter()).map(PlatformEvent::from));
        self.devices.retain(|device| &device.seat != seat);
        if (self.serial.as_ref()).is_some_and(|current| &current.seat == seat) {
            self.serial = None;
            events.push(self.selection_offered().into());
        }
        events
    }

    pub(crate) fn note_primary_press(&mut self, seat: &WlSeat, serial: ProtocolSerial) {
        if let Some(device) = self.transfers_of_mut(seat) {
            device.primary_press = Some(serial);
        }
    }

    fn transfers_of(&self, seat: &WlSeat) -> Option<&SeatTransfers> {
        (self.devices.iter()).find(|device| &device.seat == seat)
    }

    fn transfers_of_mut(&mut self, seat: &WlSeat) -> Option<&mut SeatTransfers> {
        (self.devices.iter_mut()).find(|device| &device.seat == seat)
    }
}
