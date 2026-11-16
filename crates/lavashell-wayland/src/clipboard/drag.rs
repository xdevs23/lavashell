// SPDX-License-Identifier: GPL-3.0-only
use crate::native::device::protocol_id;
use crate::sctk;
use lavashell_core::transfer::{DragAction, DragActions};
use lavashell_platform::DragDevice;
use sctk::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
};

pub(super) fn drag_device(device: &WlDataDevice) -> DragDevice {
    DragDevice(protocol_id(device))
}

pub(super) fn native_action(action: DragAction) -> DndAction {
    match action {
        DragAction::Copy => DndAction::Copy,
        DragAction::Move => DndAction::Move,
    }
}

pub(super) fn native_actions(actions: DragActions) -> DndAction {
    actions.iter().fold(DndAction::empty(), |bits, action| {
        bits | native_action(action)
    })
}

pub(crate) fn portable_action(action: DndAction) -> Option<DragAction> {
    match action {
        DndAction::Copy => Some(DragAction::Copy),
        DndAction::Move => Some(DragAction::Move),
        _ => None,
    }
}

pub(super) fn portable_actions(actions: DndAction) -> DragActions {
    DragActions::new(
        DragAction::ALL
            .into_iter()
            .filter(|action| actions.contains(native_action(*action))),
    )
}
