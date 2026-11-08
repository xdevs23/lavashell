// SPDX-License-Identifier: GPL-3.0-only
use std::io;
use std::os::fd::OwnedFd;

/// A format of clipboard data, as a MIME type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatId(pub String);

/// Identifies a selection set by the runtime. An id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionId(pub u64);

/// Identifies a read of the clipboard selection by the runtime. An id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReceiveId(pub u64);

/// Why a platform did not set a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SelectionRefusal {
    #[error("the host system does not offer a clipboard")]
    Unavailable,
    #[error("setting the clipboard needs an earlier input event")]
    NoInput,
    #[error("the input device of the selection disappeared")]
    InputDeviceGone,
}

/// Why a platform did not start a read of the selection.
#[derive(Debug, thiserror::Error)]
pub enum ReceiveRefusal {
    #[error("the host system does not offer a selection")]
    Empty,
    #[error("the selection does not have the format")]
    NoFormat,
    #[error("the pipe could not be made")]
    Pipe(#[source] io::Error),
}

/// What the runtime asks a platform to do with the clipboard.
#[derive(Debug)]
pub enum ClipboardRequest {
    /// Offers the formats as the clipboard selection. The platform then sends `ClipboardEvent::SelectionSet`.
    SetSelection(SelectionId, Vec<FormatId>),
    /// Asks the client that set the selection to write it in the format. The platform then sends
    /// `ClipboardEvent::ReceiveStarted`.
    Receive(ReceiveId, FormatId),
}

/// What happened to the clipboard.
#[derive(Debug)]
pub enum ClipboardEvent {
    /// The selection offered by the host has these formats now. The list is empty while the clipboard is empty.
    SelectionOffered(Vec<FormatId>),
    /// The platform set the selection, or the reason why it did not.
    SelectionSet(SelectionId, Result<(), SelectionRefusal>),
    /// The nonblocking read end of a pipe whose write end went to the client that set the selection, or the reason
    /// why the read did not start.
    ReceiveStarted(ReceiveId, Result<OwnedFd, ReceiveRefusal>),
    /// A client pastes the selection in the format and reads it from the nonblocking descriptor, or the platform could
    /// not prepare the descriptor.
    SourceSend(SelectionId, FormatId, io::Result<OwnedFd>),
    /// Another selection replaced the selection.
    SourceCancelled(SelectionId),
}
