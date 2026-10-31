// SPDX-License-Identifier: GPL-3.0-only
mod destination;
mod gesture;
mod handlers;

use crate::Error;
use lavashell_core::{
    geometry::Point,
    surface::SurfaceId,
    transfer::{DragAction, DragActions, DropAcceptance, ReceivedData, TransferData},
    units::Dp,
};
use sctk::{
    data_device_manager::data_offer::DragOffer,
    reexports::client::protocol::wl_data_device::WlDataDevice,
};
use smithay_client_toolkit as sctk;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransferId(pub(super) u64);

#[derive(Clone, Debug)]
pub struct DragOfferInfo {
    pub id: TransferId,
    pub surface: SurfaceId,
    pub position: Point,
    pub mime_types: Vec<String>,
    pub actions: DragActions,
    pub selected_action: Option<DragAction>,
}

#[derive(Clone, Debug)]
pub enum DragEvent {
    Enter(DragOfferInfo),
    Motion(DragOfferInfo),
    /// The drag was dropped on the surface. The drop uses the position, the MIME types and the selected action in
    /// this event. An acceptance returned for it is ignored, because the acceptance from the last enter or motion
    /// applies.
    Drop(DragOfferInfo),
    /// The drag left the surface before a drop. The acceptance returned during enter and motion no longer applies.
    Leave {
        id: TransferId,
        surface: SurfaceId,
    },
    Cancelled {
        id: TransferId,
        surface: SurfaceId,
    },
}

impl DragEvent {
    pub(crate) fn surface(&self) -> SurfaceId {
        match self {
            Self::Enter(info) | Self::Motion(info) | Self::Drop(info) => info.surface,
            Self::Leave { surface, .. } | Self::Cancelled { surface, .. } => *surface,
        }
    }
}

#[derive(Debug)]
pub struct DropEvent {
    pub id: TransferId,
    pub surface: SurfaceId,
    pub position: Point,
    pub action: DragAction,
    pub data: Result<ReceivedData, Error>,
}

/// A pointer motion while the left button pressed on `surface` is held, passed to the drag source callback of the
/// app. When the callback returns a `DragRequest`, the drag starts and that button press creates no more gestures.
#[derive(Debug)]
#[non_exhaustive]
pub struct DragGesture {
    pub surface: SurfaceId,
    pub origin: Point,
    pub position: Point,
}

impl DragGesture {
    /// Whether the pointer moved at least `distance` from `origin`, in logical pixels. A `distance` that is negative
    /// or not finite is never passed.
    #[must_use]
    pub fn passed_threshold(&self, distance: Dp) -> bool {
        distance.is_finite()
            && distance >= Dp::ZERO
            && (self.position.x - self.origin.x).hypot(self.position.y - self.origin.y) >= distance
    }
}

pub struct DragRequest {
    pub(super) data: TransferData,
    pub(super) actions: DragActions,
    pub(super) done: DragCompletion,
    pub(super) started: Option<DragStarted>,
    pub(super) received: Option<DragReceipt>,
}

pub(crate) type DragCompletion =
    Box<dyn FnOnce(Result<DragAction, Error>, &mut lavashell_core::view::View)>;

type DragStarted = Box<dyn FnOnce(&mut lavashell_core::view::View)>;
pub(super) type DragReceipt = Box<dyn FnMut(TransferId)>;

impl DragRequest {
    /// Creates a drag of `data` that allows `actions`. `done` runs with the view of the surface where the drag
    /// started. It reports the action only after every requested transfer was sent and the destination finished the
    /// drop. The drag never deletes files or app data. After a successful move, `done` removes the moved data itself.
    pub fn new(
        data: TransferData,
        actions: DragActions,
        done: impl FnOnce(Result<DragAction, Error>, &mut lavashell_core::view::View) + 'static,
    ) -> Self {
        Self {
            data,
            actions,
            done: Box::new(done),
            started: None,
            received: None,
        }
    }

    /// Sets `started`, which runs with the view of the source surface after the local checks passed and the
    /// `start_drag` request was sent. The compositor does not confirm `start_drag`, so `started` does not wait for
    /// a confirmation.
    #[must_use]
    pub fn on_start(
        mut self,
        started: impl FnOnce(&mut lavashell_core::view::View) + 'static,
    ) -> Self {
        self.started = Some(Box::new(started));
        self
    }

    pub(crate) fn on_local_receive(mut self, received: impl FnMut(TransferId) + 'static) -> Self {
        self.received = Some(Box::new(received));
        self
    }
}

pub(super) struct Incoming {
    pub device: WlDataDevice,
    pub offer: DragOffer,
    pub info: DragOfferInfo,
    pub acceptance: Option<DropAcceptance>,
}

pub(crate) fn native_action(
    action: DragAction,
) -> sctk::reexports::client::protocol::wl_data_device_manager::DndAction {
    use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
    match action {
        DragAction::Copy => DndAction::Copy,
        DragAction::Move => DndAction::Move,
    }
}

pub(crate) fn portable_action(
    action: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
) -> Option<DragAction> {
    use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
    match action {
        DndAction::Copy => Some(DragAction::Copy),
        DndAction::Move => Some(DragAction::Move),
        _ => None,
    }
}

pub(crate) fn portable_actions(
    actions: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
) -> DragActions {
    DragActions::new(
        [DragAction::Copy, DragAction::Move]
            .into_iter()
            .filter(|action| actions.contains(native_action(*action))),
    )
}
