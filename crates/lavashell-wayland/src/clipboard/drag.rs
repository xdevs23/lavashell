// SPDX-License-Identifier: GPL-3.0-only
mod destination;
mod handlers;

use super::state::{ClipboardState, SeatTransfers};
use crate::translator::Translator;
use lavashell_core::transfer::{DragAction, DragActions, TransferId};
use lavashell_platform::{DragDevice, PointerDevice};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
};
use smithay_client_toolkit as sctk;

impl Translator {
    /// Notes the serial of a press of the primary button of `pointer`, which a drag started by the app needs.
    pub(crate) fn note_primary_press(&mut self, pointer: PointerDevice, serial: u32) {
        let Some(seat) = self.pointer_seat(pointer).cloned() else {
            return;
        };
        let device = (self.clipboard.devices.iter_mut()).find(|device| device.seat == seat);
        if let Some(device) = device {
            device.primary_press = Some(serial);
        }
    }
}

impl ClipboardState {
    /// The transfers of the seat whose data device is `device`.
    fn seat_of(&mut self, device: &WlDataDevice) -> Option<&mut SeatTransfers> {
        (self.devices.iter_mut()).find(|current| current.device.inner() == device)
    }

    /// Removes and returns the drag over a surface on the seat whose data device is `device`, once it left or was
    /// dropped.
    fn take_entered(&mut self, device: &WlDataDevice) -> Option<TransferId> {
        self.seat_of(device)?.entered.take()
    }
}

pub(super) fn drag_device(device: &WlDataDevice) -> DragDevice {
    DragDevice(u64::from(device.id().protocol_id()))
}

fn native_action(action: DragAction) -> DndAction {
    match action {
        DragAction::Copy => DndAction::Copy,
        DragAction::Move => DndAction::Move,
    }
}

pub(super) fn native_actions(actions: DragActions) -> DndAction {
    [DragAction::Copy, DragAction::Move]
        .into_iter()
        .filter(|action| actions.contains(*action))
        .fold(DndAction::empty(), |bits, action| {
            bits | native_action(action)
        })
}

pub(super) fn portable_action(action: DndAction) -> Option<DragAction> {
    match action {
        DndAction::Copy => Some(DragAction::Copy),
        DndAction::Move => Some(DragAction::Move),
        _ => None,
    }
}

fn portable_actions(actions: DndAction) -> DragActions {
    DragActions::new(
        [DragAction::Copy, DragAction::Move]
            .into_iter()
            .filter(|action| actions.contains(native_action(*action))),
    )
}
