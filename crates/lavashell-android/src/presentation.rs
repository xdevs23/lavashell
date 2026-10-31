// SPDX-License-Identifier: GPL-3.0-only
use crate::hardware_buffer::HardwareBufferMemory;
use crate::native_window::NativeWindow;
use ash::khr::android_surface;
use ash::vk;
use lavashell_core::prelude::OutputRequest;
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::vulkan::FOREIGN_IMPORT_EXTENSIONS;
use lavashell_renderer::{
    Attach, ForeignMemory, PresentationBackend, RendererState, StateError, StreamBackend,
    SurfaceAlpha, VulkanRenderer,
};
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Window};
use std::ffi::CStr;
use std::sync::Arc;

/// The presentation to AOSP native windows, on a device that samples AOSP hardware buffers.
pub(crate) struct AospPresentation;

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
        }
    }

    fn supports(
        &self,
        _: &PhysicalDevice<'_>,
        _: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error> {
        // The Vulkan specification requires every queue family on AOSP to present to any native window
        // (https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html#platformQuerySupport_android).
        Ok(lavashell_vulkan::Presentation::Supported)
    }

    /// The AOSP compositor rotates the buffer for the display, so canvas and touch coordinates are in the space of the
    /// host Surface.
    fn transform(&self) -> Option<vk::SurfaceTransformFlagsKHR> {
        Some(vk::SurfaceTransformFlagsKHR::IDENTITY)
    }
}

/// An `ANativeWindow`, from which Vulkan creates a surface through `VK_KHR_android_surface`.
struct AndroidWindow(ndk::native_window::NativeWindow);

/// Attaches `window` to `state` as `surface`, with the window as the control of the surface.
pub(crate) fn attach_window<S: StreamBackend<Gpu = VulkanRenderer>>(
    state: &mut RendererState<NativeWindow, S>,
    surface: SurfaceId,
    window: NativeWindow,
    alpha: SurfaceAlpha,
) -> Result<(), StateError> {
    state.attach(Attach {
        surface,
        window: Arc::new(AndroidWindow(window.window.clone())),
        alpha,
        timing: window.timing.clone(),
        output: OutputRequest::default(),
        hdr_metadata: None,
        gpu_budget: None,
        control: window,
    })
}

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new surface of `instance` for the window, and only the `Surface` that receives the surface
// destroys it. The ndk `NativeWindow` has a reference to the `ANativeWindow`, so the window exists as long as this
// value does.
unsafe impl Window for AndroidWindow {
    fn create_surface(
        &self,
        instance: &Instance,
    ) -> Result<vk::SurfaceKHR, lavashell_vulkan::Error> {
        instance.require(&[android_surface::NAME])?;
        let functions = android_surface::Instance::new(instance.entry(), instance.ash());
        let info = vk::AndroidSurfaceCreateInfoKHR::default().window(self.0.ptr().as_ptr().cast());
        // SAFETY: ash 0.38 has no safe form of vkCreateAndroidSurfaceKHR, and no maintained crate makes a surface from
        // window handles safely. The instance has the extension enabled, as `require` checked. The handle points to a
        // live ANativeWindow while the window exists. The `Surface` contains an `Arc` of the window, so the window
        // exists as long as the surface.
        unsafe { functions.create_android_surface(&info, None) }
            .map_err(lavashell_vulkan::Error::CreateSurface)
    }
}
