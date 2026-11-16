// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
pub mod model;
mod palette;
mod tag;
pub mod ui;

pub use error::Error;
pub use tag::Tag;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));
