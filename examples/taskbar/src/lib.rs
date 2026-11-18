// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod autoplay;
mod error;
mod islands;
pub mod look;
pub mod model;
mod paint;
mod player;
pub mod script;
mod tag;
#[cfg(test)]
mod tests;

pub use autoplay::Autoplay;
pub use error::Error;
pub use player::app;

include!(concat!(env!("OUT_DIR"), "/copy.rs"));
