// SPDX-License-Identifier: GPL-3.0-only
use std::os::fd::OwnedFd;

/// Identifies a watched file descriptor. An id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WatchId(pub u64);

/// What the runtime asks a platform to do with a file descriptor.
#[derive(Debug)]
pub enum WatchRequest {
    /// Watches the descriptor until it is readable once, and closes it after that.
    Readable(WatchId, OwnedFd),
    /// Stops the watch and closes its descriptor.
    Unwatch(WatchId),
}

/// What a platform observed on a watched file descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    Readable(WatchId),
}
