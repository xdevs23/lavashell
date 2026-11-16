// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use sctk::reexports::calloop;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("a file descriptor could not join the event loop")]
    WatchFd(#[source] calloop::Error),
    #[error("a watched file descriptor could not be watched again")]
    ResumeWatch(#[source] calloop::Error),
}
