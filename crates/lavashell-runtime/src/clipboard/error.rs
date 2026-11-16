// SPDX-License-Identifier: GPL-3.0-only
use crate::pipe_transfers;
use lavashell_core::id_counter::IdsExhausted;
use lavashell_core::transfer::Cancellation;
use lavashell_platform::SelectionRefusal;
use std::io;
use std::string::FromUtf8Error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a new selection could not get an id")]
    NumberSelection(#[source] IdsExhausted),
    #[error("a new read of the clipboard could not get an id")]
    NumberReceive(#[source] IdsExhausted),
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
