// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_desktop_settings::SettingsSource;
use lavashell_desktop_settings_freedesktop::AppearanceSource;

#[must_use]
pub fn settings() -> Vec<Box<dyn SettingsSource>> {
    vec![Box::new(AppearanceSource)]
}
