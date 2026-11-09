// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers;
use lavashell_core::transfer::Cancellation;
use lavashell_platform::{DragReceiveRefusal, DragRefusal};
use std::io;

/// The failures that can occur in a drag offered to the app or started by it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new drag has a number too large for a drag id")]
    NumberDragSource,
    #[error("the platform did not start the drag")]
    StartDrag(#[source] DragRefusal),
    #[error("a drag needs a supported action")]
    WithoutAction,
    #[error("the destination asked for a format that the drag did not offer")]
    UnadvertisedFormat,
    #[error("the drag finished without a transfer and a supported action")]
    WithoutTransfer,
    #[error("the dragged data could not be sent")]
    SendDrag(#[source] io::Error),
    #[error("the dragged data could not be received")]
    ReceiveDrag(#[source] DragReceiveRefusal),
    #[error(transparent)]
    Transfer(#[from] pipe_transfers::Error),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}
