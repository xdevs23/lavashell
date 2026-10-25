// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
//! The Lucide icons for `ui.icon`, with one static for each SVG file of the lucide-static package:
//! `arrow-left.svg` becomes `ARROW_LEFT`. The build script converts the files in the `icons`
//! directory of the package found through the environment variable `LAVASHELL_LUCIDE`, and encodes
//! each icon. An icon is decoded the first time [`EncodedIcon::icon`] is called for it.

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
