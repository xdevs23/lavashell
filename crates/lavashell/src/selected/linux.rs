// SPDX-License-Identifier: GPL-3.0-only
pub use lavashell_wayland as wayland;

#[cfg_attr(feature = "desktop", path = "linux/desktop.rs")]
#[cfg_attr(not(feature = "desktop"), path = "linux/display_only.rs")]
mod platform;

pub use platform::{Platform, run};
