// SPDX-License-Identifier: GPL-3.0-only
//! Reads the appearance preferences of the Linux desktop from the Settings portal on D-Bus, without blocking the UI
//! thread.

mod portal;
#[cfg(test)]
mod tests;

use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::{ContrastPreference, PreferenceValues};

pub(crate) use portal::watch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Appearance {
    pub(crate) reduced_motion: bool,
    pub(crate) contrast: ContrastPreference,
    /// `None` while the desktop has no preferred color scheme.
    pub(crate) color_scheme: Option<ColorScheme>,
}

impl Appearance {
    pub(crate) fn applied_to(self, system: &PreferenceValues) -> PreferenceValues {
        PreferenceValues {
            reduced_motion: self.reduced_motion,
            contrast: self.contrast,
            ..system.clone()
        }
    }
}
