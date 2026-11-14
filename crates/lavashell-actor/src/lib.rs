// SPDX-License-Identifier: GPL-3.0-only
//! Generic types for actors that run on their own threads and the links between them: a mailbox whose items merge
//! while the receiver is busy, an outlet that converts items into a mailbox, a newest-wins slot, a choreographer
//! that merges overwrites until a slower consumer receives whole frames, and the clock on which a group of actors
//! measures time and waits for deadlines. The crate uses only generic types.
#![forbid(unsafe_code)]

mod actor;
mod choreographer;
mod clock;
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
pub use choreographer::{Choreographer, Frame};
pub use clock::{Activity, Clock, SystemClock, Timeout, Waiter, mailbox_on};
pub use latest::Latest;
pub use lock::{lock_ignoring_poison, wait_ignoring_poison};
pub use mailbox::{Closed, Intake, Receiver, Sender, Waker, mailbox};
pub use merge::{Effect, Merge};
pub use outlet::Outlet;
pub use spawn::{Handle, JoinError, SpawnError, spawn};
