// SPDX-License-Identifier: GPL-3.0-only
use sctk::reexports::calloop;
use sctk::reexports::client::ConnectError;
use sctk::reexports::client::backend::WaylandError;
use sctk::reexports::client::globals::{BindError, GlobalError};
use smithay_client_toolkit as sctk;

/// A failure of the connection to the compositor. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("the connection to the compositor could not be opened")]
    Connect(#[source] ConnectError),
    #[error("the globals of the compositor could not be read")]
    ReadGlobals(#[source] GlobalError),
    #[error("the requests could not be sent to the compositor")]
    Flush(#[source] WaylandError),
    #[error("wl_shm could not be bound")]
    BindShm(#[source] BindError),
    #[error("wl_compositor could not be bound")]
    BindCompositor(#[source] BindError),
}

/// A failure of the event loop that runs the Wayland connection. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("the event loop could not be created")]
    CreateEventLoop(#[source] calloop::Error),
    #[error("the Wayland connection could not join the event loop")]
    WatchConnection(#[source] calloop::Error),
    #[error("the event loop failed to dispatch")]
    DispatchEvents(#[source] calloop::Error),
}
