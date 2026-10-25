// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod model;
pub mod ui;

pub use error::Error;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));
