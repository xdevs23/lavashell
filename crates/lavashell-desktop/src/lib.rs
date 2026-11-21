// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod platform;
mod settings;

pub use platform::{DesktopPlatform, DesktopPlatformError};
pub use settings::SettingsStartError;
