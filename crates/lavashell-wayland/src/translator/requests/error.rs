// SPDX-License-Identifier: GPL-3.0-only
use lavashell_event_loop::EventLoopFailure;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum RequestsError {
    #[error("the wake for the requests of the runtime could not be created")]
    CreateRequestWake(#[source] io::Error),
    #[error("the wake source for runtime requests could not join the event loop")]
    WatchRequests(#[source] EventLoopFailure),
    #[error("the requests of the runtime could not be paused")]
    PauseRequests(#[source] EventLoopFailure),
    #[error("the requests of the runtime could not be resumed")]
    ResumeRequests(#[source] EventLoopFailure),
    #[error("the socket of the connection could not be duplicated")]
    DuplicateSocket(#[source] io::Error),
    #[error("the writability of the connection could not join the event loop")]
    WatchWritable(#[source] EventLoopFailure),
}
