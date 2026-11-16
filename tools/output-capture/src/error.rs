// SPDX-License-Identifier: GPL-3.0-only
use smithay_client_toolkit::reexports::client::{
    ConnectError, DispatchError, WEnum,
    backend::WaylandError,
    globals::{BindError, GlobalError},
    protocol::wl_shm,
};
use smithay_client_toolkit::shm::CreatePoolError;
use std::num::TryFromIntError;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("the compositor cannot be reached")]
    Connect(#[from] ConnectError),
    #[error("the globals of the compositor cannot be listed")]
    Globals(#[from] GlobalError),
    #[error("a global of the compositor cannot be bound")]
    Bind(#[from] BindError),
    #[error("the events of the compositor cannot be dispatched")]
    Dispatch(#[from] DispatchError),
    #[error("the requests cannot be sent to the compositor")]
    Flush(#[from] WaylandError),
    #[error("the request cannot be read from the standard input")]
    ReadRequest(#[source] std::io::Error),
    #[error("the line is not a request")]
    ParseRequest(#[source] serde_json::Error),
    #[error("the reply cannot be encoded")]
    EncodeReply(#[source] serde_json::Error),
    #[error("the reply cannot be written to the standard output")]
    SendReply(#[source] std::io::Error),
    #[error("the compositor has no output")]
    NoOutput,
    #[error("the compositor has more than one output")]
    AmbiguousOutput,
    #[error("the compositor offers no shared memory buffer for the frame")]
    NoSharedMemoryBuffer,
    #[error("the compositor offers the frame only in the pixel format {0:?}")]
    UnsupportedFormat(WEnum<wl_shm::Format>),
    #[error("a length of the frame does not fit the protocol")]
    FrameOutOfRange(#[source] TryFromIntError),
    #[error("the frame is larger than the address space")]
    FrameTooLarge,
    #[error("the shared memory for the frame cannot be created")]
    CreatePool(#[source] CreatePoolError),
    #[error("the shared memory for the frame cannot be enlarged")]
    ResizePool(#[source] std::io::Error),
    #[error("the compositor reported a frame as copied before the copy was requested")]
    ReadyBeforeCopy,
    #[error("the compositor could not copy the frame")]
    CopyFailed,
    #[error("the pixels of the frame cannot be written to their file")]
    WritePixels(#[source] std::io::Error),
}
