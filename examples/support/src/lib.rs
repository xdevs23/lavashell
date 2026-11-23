// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod arguments;
mod breakpoint;
mod cover;
mod error;
mod frame_log;
mod next_steps;
mod palette;
mod report;
mod samples;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod theme;

pub use arguments::{ExampleArguments, SizeArguments, parse_number};
pub use breakpoint::{Breakpoint, Layout};
pub use cover::cover;
pub use error::{Error, reject_failures};
pub use frame_log::{FrameLog, run, run_observed};
pub use lavashell_lucide as lucide;
pub use next_steps::{ExampleError, ExampleStep};
pub use palette::{TEAL_PALETTE, TealPalette};
pub use report::{exit_code, report, report_failure};
pub use samples::{HARBOR_LIGHTS, SampleTrack, UNDERTOW};
pub use theme::{Theme, ThemeChoice, ThemeSwitchColors};

use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
use std::path::PathBuf;

pub const BODY: &str = "body";
pub const DISPLAY: &str = "display";
pub const MONO: &str = "mono";

pub fn icon(encoded: &'static lucide::EncodedIcon) -> Result<&'static Icon, Error> {
    encoded.icon().map_err(Error::Icon)
}

pub fn node<'a>(view: &'a View, role: &Role, name: &str) -> Result<&'a AccessibleNode, Error> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
        .ok_or_else(|| Error::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
}

#[must_use]
pub fn size_window(window: Window, size: Option<Size>) -> Window {
    match size {
        Some(size) => window.size(size.width, size.height),
        None => window,
    }
}

pub fn default_app() -> Result<App, Error> {
    Ok(App::new().fonts(fonts()?))
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
