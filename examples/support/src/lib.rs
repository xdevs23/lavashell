// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod arguments;
mod breakpoint;
mod capture;
mod cover;
mod error;
#[cfg(target_os = "linux")]
mod frame_log;
mod platform;
mod report;
#[cfg(target_os = "linux")]
mod steps;
#[cfg(all(target_os = "linux", any(test, feature = "testing")))]
pub mod testing;

pub use arguments::{ExampleArguments, Recording, RecordingArguments, SizeArguments};
pub use breakpoint::Breakpoint;
pub use capture::write_capture;
#[cfg(target_os = "linux")]
pub use capture::write_screenshot;
pub use cover::cover;
pub use error::Error;
#[cfg(target_os = "linux")]
pub use frame_log::{run, run_observed};
pub use lavashell_lucide as lucide;
pub use lavashell_test_platform::FrameInterval;
pub use platform::platform;
pub use report::{exit_code, report, report_failure};
#[cfg(target_os = "linux")]
pub use steps::{Start, Steps, at_rest};

use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

pub const BODY: &str = "body";
pub const DISPLAY: &str = "display";
pub const MONO: &str = "mono";

/// The icon decoded from `encoded`.
///
/// # Errors
/// Returns an error when the icon does not decode.
pub fn icon(encoded: &'static lucide::EncodedIcon) -> Result<&'static Icon, Error> {
    encoded.icon().map_err(Error::Icon)
}

/// # Errors
/// Returns [`Error::MissingNode`] when the accessibility tree of `view` does not contain a node with the role `role`
/// and the name `name`.
pub fn node<'a>(view: &'a View, role: &Role, name: &str) -> Result<&'a AccessibleNode, Error> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
        .ok_or_else(|| Error::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
}

/// An app on the platform of the examples with the example fonts, without the options of the command line.
///
/// # Errors
/// Returns an error when the example fonts do not load.
#[cfg(target_os = "linux")]
pub fn default_app() -> Result<App, Error> {
    Ok(App::new(platform()).fonts(fonts()?))
}

/// The environment variable that contains the path of the directory with the font packages used by the examples.
pub const FONT_DIRECTORY: &str = "LAVASHELL_EXAMPLE_NODE_MODULES";

/// # Errors
/// Returns an error when the font directory is not set or a font does not load.
#[cfg(target_os = "linux")]
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
