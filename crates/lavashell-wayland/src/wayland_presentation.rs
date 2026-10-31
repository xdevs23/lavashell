// SPDX-License-Identifier: GPL-3.0-only
use ash::khr::wayland_surface;
use ash::vk;
use lavashell_renderer::PresentationBackend;
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Presentation};
use sctk::reexports::client::backend::Backend;
use smithay_client_toolkit as sctk;
use std::ffi::CStr;

/// The presentation backend for the surfaces on one Wayland display.
pub(crate) struct WaylandPresentation {
    display: Backend,
}

impl WaylandPresentation {
    pub(crate) fn new(display: Backend) -> Self {
        Self { display }
    }
}

impl PresentationBackend for WaylandPresentation {
    fn surface_extension(&self) -> Option<&'static CStr> {
        Some(wayland_surface::NAME)
    }

    fn features(&self) -> Features {
        Features::default()
    }

    fn supports(
        &self,
        physical: &PhysicalDevice<'_>,
        family: u32,
    ) -> Result<Presentation, lavashell_vulkan::Error> {
        let functions = wayland_functions(physical.instance())?;
        let query = functions
            .fp()
            .get_physical_device_wayland_presentation_support_khr;
        let display = self.display.display_ptr().cast();
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceWaylandPresentationSupportKHR. The ash wrapper also
        // requires a `&mut wl_display`, which cannot be made from a display shared with libwayland, so this code calls
        // the function pointer with the raw pointer. The physical device belongs to the live instance, which has
        // Wayland surfaces enabled. The pointer is the live `wl_display` of the backend stored in this value.
        let supported = unsafe { query(physical.handle(), family, display) };
        Ok(if supported == vk::TRUE {
            Presentation::Supported
        } else {
            Presentation::Unsupported
        })
    }

    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR> {
        None
    }
}

/// The Wayland surface functions loaded for `instance`.
///
/// # Errors
/// Fails when the instance lacks Wayland surfaces.
pub(crate) fn wayland_functions(
    instance: &Instance,
) -> Result<wayland_surface::Instance, lavashell_vulkan::Error> {
    instance.require(&[wayland_surface::NAME])?;
    Ok(wayland_surface::Instance::new(
        instance.entry(),
        instance.ash(),
    ))
}
