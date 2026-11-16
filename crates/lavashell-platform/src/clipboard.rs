// SPDX-License-Identifier: GPL-3.0-only
use std::io;
use std::os::fd::OwnedFd;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReceiveId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SelectionRefusal {
    #[error("the host system does not offer a clipboard")]
    Unavailable,
    #[error("setting the clipboard needs an earlier input event")]
    NoInput,
    #[error("the input device of the selection disappeared")]
    InputDeviceGone,
}

#[derive(Debug, thiserror::Error)]
pub enum ReceiveRefusal {
    #[error("the host system does not offer a selection")]
    Empty,
    #[error("the selection does not have the format")]
    NoFormat,
    #[error("the pipe could not be made")]
    Pipe(#[source] io::Error),
}

#[derive(Debug)]
pub enum ClipboardRequest {
    SetSelection(SelectionId, Vec<FormatId>),
    Receive(ReceiveId, FormatId),
}

#[derive(Debug)]
pub enum ClipboardEvent {
    SelectionOffered(Vec<FormatId>),
    SelectionSet(SelectionId, Result<(), SelectionRefusal>),
    ReceiveStarted(ReceiveId, Result<OwnedFd, ReceiveRefusal>),
    SourceSend(SelectionId, FormatId, io::Result<OwnedFd>),
    SourceCancelled(SelectionId),
}
