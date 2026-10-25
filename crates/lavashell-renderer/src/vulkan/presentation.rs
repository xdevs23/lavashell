// SPDX-License-Identifier: GPL-3.0-only
use lavashell_vulkan::{Features, PhysicalDevice, Surface};
use std::ffi::CStr;

/// The device extensions that a swapchain uses to present frames, to send HDR metadata with them and to time them.
pub(super) const SWAPCHAIN_EXTENSIONS: [&CStr; 3] = [
    ash::khr::swapchain::NAME,
    ash::ext::hdr_metadata::NAME,
    ash::google::display_timing::NAME,
];

/// The platform on which the renderer presents its frames, with a window system or offscreen only.
pub(super) trait Presentation {
    /// The instance extension that creates the surfaces of this platform, or `None` for a renderer that draws
    /// offscreen only.
    fn surface_extension(&self) -> Option<&'static CStr>;

    /// The device extensions that the renderer enables on this platform where the device offers them.
    fn device_extensions(&self) -> Vec<&'static CStr>;

    /// The device features that the renderer enables on this platform where the device offers them.
    fn features(&self) -> Features;

    /// Whether the queue family `family` of `physical` can present to the surfaces of this platform.
    ///
    /// # Errors
    /// Fails when the query to the driver fails.
    fn supports(
        &self,
        physical: &PhysicalDevice<'_>,
        family: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error>;
}

pub(super) struct Headless;

impl Presentation for Headless {
    fn surface_extension(&self) -> Option<&'static CStr> {
        None
    }

    fn device_extensions(&self) -> Vec<&'static CStr> {
        Vec::new()
    }

    fn features(&self) -> Features {
        Features::default()
    }

    fn supports(
        &self,
        _: &PhysicalDevice<'_>,
        _: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error> {
        Ok(lavashell_vulkan::Presentation::Supported)
    }
}

/// The instance extensions needed by surfaces that `surface_extension` creates, plus the extension for extended
/// swapchain color spaces when `extended` is true. Without a surface extension, the list is empty.
pub(super) fn instance_extensions(
    surface_extension: Option<&'static CStr>,
    extended: bool,
) -> Vec<&'static CStr> {
    let Some(surface_extension) = surface_extension else {
        return Vec::new();
    };
    let mut extensions = Surface::instance_extensions(surface_extension).to_vec();
    if extended {
        extensions.push(ash::ext::swapchain_colorspace::NAME);
    }
    extensions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_does_not_require_a_window_system() {
        assert_eq!(instance_extensions(None, true), Vec::<&CStr>::new());
    }

    #[test]
    fn presentation_only_enables_its_own_window_system() {
        for platform in [
            ash::khr::wayland_surface::NAME,
            ash::khr::android_surface::NAME,
        ] {
            assert_eq!(
                instance_extensions(Some(platform), false),
                [ash::khr::surface::NAME, platform],
            );
            assert_eq!(
                instance_extensions(Some(platform), true),
                [
                    ash::khr::surface::NAME,
                    platform,
                    ash::ext::swapchain_colorspace::NAME
                ],
            );
        }
    }
}
