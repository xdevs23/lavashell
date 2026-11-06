// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::transfer::Cancellation;
use lavashell_core::units::NumberError;
use lavashell_platform::{DragReceiveRefusal, DragRefusal, SelectionRefusal};
use std::collections::TryReserveError;
use std::io;
use std::string::FromUtf8Error;

/// A failure of the runtime, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("every surface id is used")]
    NumberSurface,
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("a new selection has a number too large for a selection id")]
    NumberSelection,
    #[error("a new read of the clipboard has a number too large for a receive id")]
    NumberReceive,
    #[error("a new drag has a number too large for a drag id")]
    NumberDragSource,
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
    #[error("the platform did not set the clipboard selection")]
    SetSelection(#[source] SelectionRefusal),
    #[error("the clipboard is empty")]
    ClipboardEmpty,
    #[error("the clipboard has none of the requested formats")]
    NoRequestedFormat,
    #[error("the pipe of a clipboard transfer could not be created")]
    CreatePipe(#[source] io::Error),
    #[error("the transfer pipe could not be watched")]
    WatchPipe(#[source] io::Error),
    #[error("the transfer pipe could not be read")]
    ReadPipe(#[source] io::Error),
    #[error("the transfer pipe could not be written")]
    WritePipe(#[source] io::Error),
    #[error("the memory for the transfer could not be reserved")]
    ReserveTransfer(#[source] TryReserveError),
    #[error("the transferred text is not UTF-8")]
    DecodeText(#[source] FromUtf8Error),
    #[error("the platform did not start the drag")]
    StartDrag(#[source] DragRefusal),
    #[error("a drag needs a supported action")]
    DragWithoutAction,
    #[error("the destination asked for a format that the drag did not offer")]
    UnadvertisedFormat,
    #[error("the drag finished without a transfer and a supported action")]
    DragWithoutTransfer,
    #[error("the dragged data could not be sent")]
    SendDrag(#[source] io::Error),
    #[error("the dragged data could not be received")]
    ReceiveDrag(#[source] DragReceiveRefusal),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}
