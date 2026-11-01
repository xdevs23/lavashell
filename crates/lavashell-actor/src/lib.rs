// SPDX-License-Identifier: GPL-3.0-only
//! A newest-wins slot for handing values between threads that run at different speeds. The crate uses only generic
//! types.
#![forbid(unsafe_code)]

mod latest;

pub use latest::Latest;
