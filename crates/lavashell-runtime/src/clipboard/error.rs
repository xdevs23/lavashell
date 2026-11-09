// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers;
use lavashell_core::transfer::Cancellation;
use lavashell_platform::SelectionRefusal;
use std::io;
use std::string::FromUtf8Error;

/// The failures that can occur while putting data on the clipboard and reading data from it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new selection has a number too large for a selection id")]
    NumberSelection,
    #[error("a new read of the clipboard has a number too large for a receive id")]
    NumberReceive,
    #[error("the platform did not set the clipboard selection")]
    SetSelection(#[source] SelectionRefusal),
    #[error("the clipboard is empty")]
    Empty,
    #[error("the clipboard has none of the requested formats")]
    NoRequestedFormat,
    #[error("the pipe of a clipboard transfer could not be created")]
    CreatePipe(#[source] io::Error),
    #[error("the transferred text is not UTF-8")]
    DecodeText(#[source] FromUtf8Error),
    #[error(transparent)]
    Transfer(#[from] pipe_transfers::Error),
    #[error("the transfer was cancelled")]
    Cancelled(#[from] Cancellation),
}
