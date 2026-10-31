// SPDX-License-Identifier: GPL-3.0-only
use ash::vk;
use lavashell_vulkan::{Features, PhysicalDevice, Surface};
use std::ffi::CStr;

/// The device extensions that a swapchain uses to present frames, to send HDR metadata with them and to time them.
const SWAPCHAIN_EXTENSIONS: [&CStr; 3] = [
    ash::khr::swapchain::NAME,
    ash::ext::hdr_metadata::NAME,
    ash::google::display_timing::NAME,
];

/// The presentation backend on which the renderer presents frames, with a window system or offscreen only.
pub trait PresentationBackend {
    /// The instance extension that creates surfaces for this backend, or `None` for a renderer that draws offscreen
    /// only.
    fn surface_extension(&self) -> Option<&'static CStr>;

    /// The device extensions that the renderer enables for this backend, beyond the swapchain extensions, where the
    /// device offers them.
    fn additional_device_extensions(&self) -> Vec<&'static CStr> {
        Vec::new()
    }

    /// The device features that the renderer enables for this backend where the device offers them.
    fn features(&self) -> Features;

    /// Whether the queue family `family` of `physical` can present to surfaces created for this backend.
    ///
    /// # Errors
    /// Fails when the query to the driver fails.
    fn supports(
        &self,
        physical: &PhysicalDevice<'_>,
        family: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error>;

    /// The transform used by every swapchain on this backend. Returns `None` when each swapchain uses the transform
    /// returned by its surface.
    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR>;
}

pub(super) struct Headless;

impl PresentationBackend for Headless {
    fn surface_extension(&self) -> Option<&'static CStr> {
        None
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

    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR> {
        None
    }
}

/// The device extensions that the renderer enables for `backend` where the device offers them: the swapchain
/// extensions for a backend with surfaces, plus the additional extensions of `backend`.
pub(super) fn device_extensions(backend: &impl PresentationBackend) -> Vec<&'static CStr> {
    let swapchain = if backend.surface_extension().is_some() {
        SWAPCHAIN_EXTENSIONS.as_slice()
    } else {
        &[]
    };
    [swapchain, backend.additional_device_extensions().as_slice()].concat()
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
    fn presentation_only_enables_its_own_surface_extension() {
        for surface_extension in [ash::ext::headless_surface::NAME, ash::khr::display::NAME] {
            assert_eq!(
                instance_extensions(Some(surface_extension), false),
                [ash::khr::surface::NAME, surface_extension],
            );
            assert_eq!(
                instance_extensions(Some(surface_extension), true),
                [
                    ash::khr::surface::NAME,
                    surface_extension,
                    ash::ext::swapchain_colorspace::NAME
                ],
            );
        }
    }
}
