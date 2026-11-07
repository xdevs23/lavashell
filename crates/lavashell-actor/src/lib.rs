// SPDX-License-Identifier: GPL-3.0-only
//! Generic types for actors that run on their own threads and the links between them: a mailbox whose items merge
//! while the receiver is busy, a newest-wins slot, and a choreographer that merges overwrites until a slower consumer
//! takes whole frames. The crate uses only generic types.
#![forbid(unsafe_code)]

mod actor;
mod choreographer;
mod latest;
mod lock;
mod mailbox;
mod merge;
mod pending;
mod spawn;
#[cfg(test)]
mod test_items;

pub use actor::{Actor, Flow};
pub use choreographer::{Choreographer, Frame};
pub use latest::Latest;
pub use lock::lock_ignoring_poison;
pub use mailbox::{Closed, Intake, Receiver, Sender, mailbox};
pub use merge::{Effect, Merge};
pub use spawn::{Handle, JoinError, SpawnError, spawn};
