// SPDX-License-Identifier: GPL-3.0-only
pub use lavashell_wayland as wayland;

#[cfg(feature = "desktop")]
pub type Platform = lavashell_desktop::DesktopPlatform<lavashell_wayland::Wayland>;

#[cfg(not(feature = "desktop"))]
pub type Platform = lavashell_wayland::Wayland;
