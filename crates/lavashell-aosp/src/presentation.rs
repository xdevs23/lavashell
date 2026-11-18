// SPDX-License-Identifier: GPL-3.0-only
use crate::hardware_buffer::HardwareBufferMemory;
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use ash::khr::android_surface;
use ash::vk;
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Window};
use lavashell_vulkan_renderer::vulkan::FOREIGN_IMPORT_EXTENSIONS;
use lavashell_vulkan_renderer::{ForeignMemory, PresentationBackend, VulkanBackend, VulkanWindow};
use std::ffi::CStr;
use std::sync::Arc;

pub(crate) type AospBackend = VulkanBackend<AospPresentation, AospStreams>;

pub struct AospPresentation;

impl PresentationBackend for AospPresentation {
    fn surface_extension(&self) -> Option<&'static CStr> {
        Some(android_surface::NAME)
    }

    fn additional_device_extensions(&self) -> Vec<&'static CStr> {
        [
            FOREIGN_IMPORT_EXTENSIONS.as_slice(),
            HardwareBufferMemory::EXTENSIONS,
        ]
        .concat()
    }

    fn features(&self) -> Features {
        Features {
            sampler_ycbcr_conversion: true,
            ..Features::default()
        }
    }

    fn supports(
        &self,
        _: &PhysicalDevice<'_>,
        _: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error> {
        // https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#_querying_for_wsi_support
        Ok(lavashell_vulkan::Presentation::Supported)
    }

    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR> {
        Some(vk::SurfaceTransformFlagsKHR::IDENTITY)
    }
}

struct AospWindow(ndk::native_window::NativeWindow);

pub(crate) fn vulkan_window(window: &NativeWindow) -> VulkanWindow {
    Arc::new(AospWindow(window.window.clone()))
}

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new surface of `instance` for the window, and only the `Surface` that receives the surface
// destroys it. The ndk `NativeWindow` has a reference to the `ANativeWindow`, so the window exists as long as this
// value does.
unsafe impl Window for AospWindow {
    fn create_surface(
        &self,
        instance: &Instance,
    ) -> Result<vk::SurfaceKHR, lavashell_vulkan::Error> {
        instance.require(&[android_surface::NAME])?;
        let functions = android_surface::Instance::new(instance.entry(), instance.ash());
        let info = vk::AndroidSurfaceCreateInfoKHR::default().window(self.0.ptr().as_ptr().cast());
        // SAFETY: ash 0.38 has no safe form of the call that creates a surface of a native window, and no maintained
        // crate makes a surface from window handles safely. The instance has the extension enabled, as `require`
        // checked. The handle points to a live ANativeWindow while the window exists. The `Surface` contains an `Arc`
        // of the window, so the window exists as long as the surface.
        unsafe { functions.create_android_surface(&info, None) }
            .map_err(lavashell_vulkan::Error::CreateSurface)
    }
}
