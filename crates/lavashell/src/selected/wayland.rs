// SPDX-License-Identifier: GPL-3.0-only
pub use lavashell_wayland as wayland;

pub type Platform = lavashell_wayland::Wayland;

#[must_use]
pub fn setup() -> lavashell_wayland::WaylandSetup {
    lavashell_wayland::WaylandSetup {
        settings: lavashell_desktop::settings(),
    }
}
