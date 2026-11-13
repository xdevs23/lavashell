// SPDX-License-Identifier: GPL-3.0-only
use sctk::reexports::calloop;
use smithay_client_toolkit as sctk;

/// A failure of a watched file descriptor. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] calloop::Error),
    #[error("a watched file descriptor could not be watched again")]
    ResumeWatch(#[source] calloop::Error),
}
