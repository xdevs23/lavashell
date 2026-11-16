// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod actor;
mod clock;
mod frame_batcher;
mod latest;
mod lock;
mod mailbox;
mod merge;
mod outlet;
mod pending;
mod spawn;
#[cfg(test)]
mod test_items;

pub use actor::{Actor, Flow};
pub use clock::{Activity, Clock, SystemClock, Timeout, Waiter, mailbox_on};
pub use frame_batcher::{Frame, FrameBatcher};
pub use latest::{Latest, Publication};
pub use lock::{
    LockPoisoned, lock_ignoring_poison, lock_unpoisoned, wait_ignoring_poison, wait_unpoisoned,
};
pub use mailbox::{Closed, Intake, Receiver, Sender, Waker, mailbox};
pub use merge::{Merge, MergeEffect};
pub use outlet::Outlet;
pub use spawn::{Handle, JoinError, SpawnError, spawn};
