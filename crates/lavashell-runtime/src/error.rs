// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::accessibility::ActionError;
use lavashell_core::transfer::Cancellation;
use lavashell_core::units::NumberError;
use lavashell_platform::SelectionRefusal;
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
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
    #[error("the session-lock handle belongs to another lock")]
    SessionLockReused,
    #[error("a session lock needs an output")]
    SessionLockWithoutOutput,
    #[error("the host refused or ended the session lock")]
    SessionLockEnded,
    #[error("the content of a panel measures a length that is negative or not finite")]
    PanelContent(#[source] NumberError),
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
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}

/// A failure while the runtime handles an event of an assistive technology. `E` is the failure of the platform to
/// show a frame.
#[derive(Debug, thiserror::Error)]
pub enum AccessibilityFailure<E> {
    #[error("a surface could not show the frame that an assistive technology reads")]
    Show(#[source] E),
    #[error("the app could not perform an action requested by an assistive technology")]
    PerformAction(#[source] ActionError),
}
