// SPDX-License-Identifier: GPL-3.0-only
use smithay_client_toolkit::reexports::client::{
    ConnectError, DispatchError,
    backend::WaylandError,
    globals::{BindError, GlobalError},
};
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
    #[error("the file that shares the keymap cannot be created in the runtime directory")]
    CreateKeymapFile(#[source] std::io::Error),
    #[error("the keymap cannot be written to its file")]
    WriteKeymap(#[source] std::io::Error),
    #[error("the keymap is too large for the protocol")]
    KeymapTooLarge(#[source] TryFromIntError),
    #[error("the size of the outputs does not fit the protocol")]
    ExtentOutOfRange(#[source] TryFromIntError),
    #[error("the compositor has no seat")]
    NoSeat,
    #[error("the compositor has more than one seat")]
    AmbiguousSeat,
    #[error("the compositor has no output")]
    NoOutput,
    #[error("the compositor reports no logical position or size of an output")]
    OutputWithoutLogicalRectangle,
    #[error("libxkbcommon cannot read the xkeyboard-config data in {0}")]
    KeymapData(&'static str),
    #[error(
        "libxkbcommon cannot compile the keymap for the default names from the xkeyboard-config data in {0}"
    )]
    Keymap(&'static str),
    #[error("the keymap has no key named {0:?}")]
    UnknownKey(String),
    #[error("the runtime directory is not set")]
    RuntimeDirectoryMissing,
    #[error("the keyboard was not enabled")]
    KeyboardDisabled,
}
