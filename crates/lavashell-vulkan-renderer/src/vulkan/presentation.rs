// SPDX-License-Identifier: GPL-3.0-only
use ash::vk;
use lavashell_vulkan::{Features, PhysicalDevice, Surface, Swapchain};
use std::ffi::CStr;

const OPTIONAL_SWAPCHAIN_EXTENSIONS: [&CStr; 2] = [
    ash::ext::hdr_metadata::NAME,
    ash::google::display_timing::NAME,
];

pub trait PresentationBackend {
    fn surface_extension(&self) -> Option<&'static CStr>;

    fn additional_device_extensions(&self) -> Vec<&'static CStr> {
        Vec::new()
    }

    fn features(&self) -> Features;

    fn supports(
        &self,
        physical: &PhysicalDevice<'_>,
        family: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error>;

    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR>;
}

pub struct HeadlessPresentation {
    surface_extension: Option<&'static CStr>,
}

impl HeadlessPresentation {
    #[must_use]
    pub fn with_surfaces() -> Self {
        Self {
            surface_extension: Some(ash::ext::headless_surface::NAME),
        }
    }

    #[must_use]
    pub fn without_surfaces() -> Self {
        Self {
            surface_extension: None,
        }
    }
}

impl PresentationBackend for HeadlessPresentation {
    fn surface_extension(&self) -> Option<&'static CStr> {
        self.surface_extension
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

pub(super) fn device_extensions(backend: &impl PresentationBackend) -> Vec<&'static CStr> {
    let swapchain = if backend.surface_extension().is_some() {
        [
            Swapchain::device_extensions().as_slice(),
            &OPTIONAL_SWAPCHAIN_EXTENSIONS,
        ]
        .concat()
    } else {
        Vec::new()
    };
    [swapchain, backend.additional_device_extensions()].concat()
}

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
        let surfaces = [ash::ext::headless_surface::NAME, ash::khr::display::NAME];
        for (surface_extension, other) in [(surfaces[0], surfaces[1]), (surfaces[1], surfaces[0])] {
            for extended in [false, true] {
                let extensions = instance_extensions(Some(surface_extension), extended);
                assert!(extensions.contains(&surface_extension));
                assert!(!extensions.contains(&other));
                assert_eq!(
                    extensions.contains(&ash::ext::swapchain_colorspace::NAME),
                    extended
                );
            }
        }
    }
}
