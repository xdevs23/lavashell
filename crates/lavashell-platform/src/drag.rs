// SPDX-License-Identifier: GPL-3.0-only
use crate::{FormatId, PointerDevice};
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{
    Cancellation, DragAction, DragActions, DragOfferInfo, DropAcceptance, TransferId,
};
use std::io;
use std::os::fd::OwnedFd;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DragDevice(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DragSourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DragRefusal {
    #[error("the host system does not offer drag and drop")]
    Unavailable,
    #[error("the host system cannot report the end of a drag")]
    CompletionUnavailable,
    #[error("the pointer of the drag disappeared")]
    InputDeviceGone,
    #[error("the drag needs a press of the primary button")]
    NoPress,
    #[error("the surface of the drag closed")]
    SurfaceClosed,
}

#[derive(Debug, thiserror::Error)]
pub enum DragReceiveRefusal {
    #[error("the platform does not know the dropped drag")]
    UnknownOffer,
    #[error("the pipe could not be made")]
    Pipe(#[source] io::Error),
}

#[derive(Debug)]
pub enum DragTargetEvent {
    Enter {
        device: DragDevice,
        offer: DragOfferInfo,
    },
    Motion(TransferId, Point),
    ActionsChanged(TransferId, DragActions),
    ActionSelected(TransferId, Option<DragAction>),
    Leave(TransferId),
    Drop(TransferId),
    ReceiveStarted(TransferId, Result<OwnedFd, DragReceiveRefusal>),
    Cancelled(TransferId),
}

impl Merge for DragTargetEvent {
    type Key = TransferId;

    fn effect(&self) -> MergeEffect<TransferId> {
        match self {
            Self::Motion(drag, _) => MergeEffect::Overwrites(*drag),
            _ => MergeEffect::Appends,
        }
    }
}

#[derive(Debug)]
pub enum DragTargetRequest {
    Answer(TransferId, Option<DropAcceptance>),
    Receive(TransferId, FormatId),
    Finish(TransferId),
    Release(TransferId),
}

#[derive(Debug)]
pub enum DragSourceRequest {
    Start {
        drag: DragSourceId,
        pointer: PointerDevice,
        surface: SurfaceId,
        formats: Vec<FormatId>,
        actions: DragActions,
    },
    Cancel(DragSourceId),
}

#[derive(Debug)]
pub enum DragSourceEvent {
    Started(DragSourceId, Result<DragDevice, DragRefusal>),
    Send {
        drag: DragSourceId,
        format: FormatId,
        pipe: io::Result<OwnedFd>,
        local: Option<TransferId>,
    },
    ActionSelected(DragSourceId, Option<DragAction>),
    Finished(DragSourceId),
    Cancelled(DragSourceId, Cancellation),
}
