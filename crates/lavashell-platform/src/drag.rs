// SPDX-License-Identifier: GPL-3.0-only
use crate::{FormatId, PointerDevice};
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::transfer::{
    Cancellation, DragAction, DragActions, DragOfferInfo, DropAcceptance, TransferId,
};
use std::io;
use std::os::fd::OwnedFd;

/// Identifies the host device used by drags, such as the input devices of one seat, while it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DragDevice(pub u64);

/// Identifies a drag started by the runtime. An id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DragSourceId(pub u64);

/// Why a platform did not start a drag.
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

/// Why a platform did not start the read of a dropped drag.
#[derive(Debug, thiserror::Error)]
pub enum DragReceiveRefusal {
    #[error("the platform does not know the dropped drag")]
    UnknownOffer,
    #[error("the pipe could not be made")]
    Pipe(#[source] io::Error),
}

/// What a drag offered to the app did. Every event after `Enter` refers to the drag by the id in its offer.
#[derive(Debug)]
pub enum DragTargetEvent {
    /// A drag on `device` entered an open surface.
    Enter {
        device: DragDevice,
        offer: DragOfferInfo,
    },
    /// The drag moved to the position on its surface. A motion replaces the earlier motion of the drag.
    Motion(TransferId, Point),
    /// The source of the drag allows these actions now.
    ActionsChanged(TransferId, DragActions),
    /// The host selected the action for a drop, or none.
    ActionSelected(TransferId, Option<DragAction>),
    /// The drag left its surface without a drop.
    Leave(TransferId),
    Drop(TransferId),
    /// The nonblocking read end of a pipe to which the source writes the dropped data, or the reason why the read did
    /// not start.
    ReceiveStarted(TransferId, Result<OwnedFd, DragReceiveRefusal>),
    /// The device used by the drag disappeared, before or after the drop.
    Cancelled(TransferId),
}

/// What the runtime asks a platform to do with a drag offered to the app.
#[derive(Debug)]
pub enum DragTargetRequest {
    /// Accepts a drop in the format and with the action of the acceptance, or rejects the drop for `None`. The
    /// answer replaces the earlier answer for the drag.
    Answer(TransferId, Option<DropAcceptance>),
    /// Requests that the source of the dropped drag write the data in the format. The platform then sends
    /// `DragTargetEvent::ReceiveStarted`.
    Receive(TransferId, FormatId),
    /// Reports to the source that the drop succeeded, and releases the dropped drag.
    Finish(TransferId),
    /// Releases the dropped drag without a successful drop.
    Release(TransferId),
}

/// What the runtime asks a platform to do with a drag started by the app.
#[derive(Debug)]
pub enum DragSourceRequest {
    /// Starts a drag of data in `formats` from `surface` with `pointer`, whose primary button is down. The platform
    /// then sends `DragSourceEvent::Started`.
    Start {
        drag: DragSourceId,
        pointer: PointerDevice,
        surface: SurfaceId,
        formats: Vec<FormatId>,
        actions: DragActions,
    },
    /// Ends the drag before the destination finished the drop.
    Cancel(DragSourceId),
}

/// What happened to a drag started by the app.
#[derive(Debug)]
pub enum DragSourceEvent {
    /// The drag started on the device, or the reason why it did not start.
    Started(DragSourceId, Result<DragDevice, DragRefusal>),
    /// A destination reads the dragged data in `format` from the nonblocking descriptor `pipe`, or the platform could
    /// not prepare the descriptor. `local` is the drag offered to this app that reads from `pipe`, when the platform
    /// can tell.
    Send {
        drag: DragSourceId,
        format: FormatId,
        pipe: io::Result<OwnedFd>,
        local: Option<TransferId>,
    },
    /// The destination selected the action, or none.
    ActionSelected(DragSourceId, Option<DragAction>),
    /// The destination finished the drop.
    Finished(DragSourceId),
    /// The drag ended before the destination finished the drop.
    Cancelled(DragSourceId, Cancellation),
}
