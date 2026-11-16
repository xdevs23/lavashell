// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Merge, MergeEffect};
use std::os::fd::OwnedFd;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Readable,
    Writable,
}

#[derive(Debug)]
pub enum WatchRequest {
    Watch(WatchId, OwnedFd, Readiness),
    Resume(WatchId),
    Unwatch(WatchId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    Ready(WatchId),
}

impl Merge for WatchEvent {
    type Key = WatchId;

    fn effect(&self) -> MergeEffect<WatchId> {
        match *self {
            Self::Ready(watch) => MergeEffect::Overwrites(watch),
        }
    }
}
