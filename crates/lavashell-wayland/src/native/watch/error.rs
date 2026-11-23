// SPDX-License-Identifier: GPL-3.0-only
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_event_loop::EventLoopFailure;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] EventLoopFailure),
    #[error("a watched file descriptor could not be watched again")]
    ResumeWatch(#[source] EventLoopFailure),
}

impl NamesNextStep for WatchError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::WatchFd(failure) | Self::ResumeWatch(failure) => failure.next_step(),
        }
    }
}
