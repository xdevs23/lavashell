// SPDX-License-Identifier: GPL-3.0-only
use std::os::fd::OwnedFd;

/// Identifies a watched file descriptor. An id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchId(pub u64);

/// What a watch waits for on its file descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Readable,
    Writable,
}

/// What the runtime asks a platform to do with a file descriptor.
#[derive(Debug)]
pub enum WatchRequest {
    /// Watches the descriptor and reports when it is ready, until `Unwatch` closes it. After each report the watch
    /// waits for `Resume`, so a descriptor that is still ready is reported once per handling.
    Watch(WatchId, OwnedFd, Readiness),
    /// Reports the watch again once its descriptor is ready.
    Resume(WatchId),
    /// Stops the watch and closes its descriptor.
    Unwatch(WatchId),
}

/// What a platform observed on a watched file descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    Ready(WatchId),
}
