// SPDX-License-Identifier: GPL-3.0-only
use lavashell_event_loop::EventLoopFailure;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] EventLoopFailure),
    #[error("a watched file descriptor could not be watched again")]
    ResumeWatch(#[source] EventLoopFailure),
}
