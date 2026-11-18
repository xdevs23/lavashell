// SPDX-License-Identifier: GPL-3.0-only
use lavashell_desktop_settings::SettingsSource;

pub struct WaylandSetup {
    pub settings: Vec<Box<dyn SettingsSource>>,
}
