// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod arguments;
mod breakpoint;
mod capture;
mod cover;
mod error;
mod frame_log;
mod next_steps;
mod palette;
mod platform;
mod report;
mod samples;
mod steps;
#[cfg(feature = "testing")]
pub mod testing;
mod theme;

pub use arguments::{ExampleArguments, Recording, RecordingArguments, SizeArguments};
pub use breakpoint::{Breakpoint, Layout};
pub use capture::{Capture, OffscreenView, write_capture};
pub use cover::cover;
pub use error::{Error, without_failures};
pub use frame_log::{run, run_observed};
pub use lavashell_lucide as lucide;
pub use lavashell_test_platform::FrameInterval;
pub use next_steps::{ExampleError, NextStep};
pub use palette::{TEAL_PALETTE, TealPalette};
pub use platform::platform;
pub use report::{exit_code, report, report_failure};
pub use samples::{HARBOR_LIGHTS, SampleTrack, UNDERTOW};
pub use steps::Steps;
pub use theme::{Theme, ThemeChoice, ThemeSwitchColors};

use lavashell::prelude::*;
use std::path::PathBuf;

pub const BODY: &str = "body";
pub const DISPLAY: &str = "display";
pub const MONO: &str = "mono";

pub fn icon(encoded: &'static lucide::EncodedIcon) -> Result<&'static Icon, Error> {
    encoded.icon().map_err(Error::Icon)
}

pub const FONT_DIRECTORY: &str = "LAVASHELL_EXAMPLE_NODE_MODULES";

pub fn fonts() -> Result<Fonts, Error> {
    let dependencies = std::env::var_os(FONT_DIRECTORY)
        .map(PathBuf::from)
        .ok_or(Error::FontDirectory)?;
    [
        (BODY, "dm-sans"),
        (DISPLAY, "manrope"),
        (MONO, "jetbrains-mono"),
    ]
    .into_iter()
    .try_fold(Fonts::new(), |fonts, (alias, family)| {
        let file = format!("@fontsource-variable/{family}/files/{family}-latin-wght-normal.woff2");
        fonts.file(alias, dependencies.join(file))
    })
    .map_err(Error::Fonts)
}

#[cfg(test)]
mod tests;
