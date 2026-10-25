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
    #[error("the keymap or the commands cannot be read or written")]
    Io(#[from] std::io::Error),
    #[error("the line is not a command")]
    Command(#[from] clap::Error),
    #[error("a size does not fit the protocol")]
    Size(#[from] TryFromIntError),
    #[error("the compositor has no seat")]
    NoSeat,
    #[error("the compositor has no output")]
    NoOutput,
    #[error("the compositor reports no logical position or size of an output")]
    OutputWithoutLogicalRectangle,
    #[error(
        "libxkbcommon cannot read the xkeyboard-config data in {0}. Build the driver again inside `nix develop`."
    )]
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
