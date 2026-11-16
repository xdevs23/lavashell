// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

pub use decode::{EncodedIcon, Error};
pub use icons::*;

mod decode;
#[path = "../outline.rs"]
mod outline;

mod icons {
    use crate::decode::{EncodedIcon, icon};
    use std::sync::LazyLock;

    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

#[cfg(test)]
#[path = "../convert.rs"]
mod convert;
#[cfg(test)]
mod tests;
