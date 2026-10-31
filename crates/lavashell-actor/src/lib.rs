// SPDX-License-Identifier: GPL-3.0-only
//! Generic types for handing values between threads that run at different speeds: a newest-wins slot, and a
//! choreographer that merges overwrites until a slower consumer takes whole frames. The crate uses only generic
//! types.
#![forbid(unsafe_code)]

mod choreographer;
mod latest;
mod merge;

pub use choreographer::{Choreographer, Frame};
pub use latest::Latest;
pub use merge::{Effect, Merge};
