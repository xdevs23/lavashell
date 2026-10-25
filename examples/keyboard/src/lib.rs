// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
mod look;
pub mod model;
pub mod scene;
pub mod stage;

pub use error::Error;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));
