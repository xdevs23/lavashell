// SPDX-License-Identifier: GPL-3.0-only
use sctk::reexports::calloop;
use smithay_client_toolkit as sctk;
use std::io;

/// A failure while the host receives requests from the runtime. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum RequestsError {
    #[error("the wake for the requests of the runtime could not be created")]
    CreateRequestWake(#[source] io::Error),
    #[error("the wake source for runtime requests could not join the event loop")]
    WatchRequests(#[source] calloop::Error),
    #[error("the requests of the runtime could not be paused")]
    PauseRequests(#[source] calloop::Error),
    #[error("the requests of the runtime could not be resumed")]
    ResumeRequests(#[source] calloop::Error),
    #[error("the socket of the connection could not be duplicated")]
    DuplicateSocket(#[source] io::Error),
    #[error("the writability of the connection could not join the event loop")]
    WatchWritable(#[source] calloop::Error),
}
