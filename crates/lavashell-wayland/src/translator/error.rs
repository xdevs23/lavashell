// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use lavashell_event_loop::EventLoopFailure;
use sctk::reexports::client::ConnectError;
use sctk::reexports::client::backend::WaylandError;
use sctk::reexports::client::globals::{BindError, GlobalError};

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

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("the event loop could not be created")]
    CreateEventLoop(#[source] EventLoopFailure),
    #[error("the Wayland connection could not join the event loop")]
    WatchConnection(#[source] EventLoopFailure),
    #[error("the event loop failed to dispatch")]
    DispatchEvents(#[source] EventLoopFailure),
}
