// SPDX-License-Identifier: GPL-3.0-only
use super::{
    FOREIGN_IMPORT_EXTENSIONS, FrameOutcome, SurfaceAlpha, SurfaceId, SurfaceTarget, SurfaceTiming,
    VulkanRenderer, presentation, wsi,
};
use crate::Error;
use ash::android::external_memory_android_hardware_buffer as hardware_buffer;
use ash::khr::android_surface;
use ash::vk;
use lavashell_vulkan::{ExtensionSet, Features, Instance, PhysicalDevice, Window};
use ndk::native_window::NativeWindow;
use skia_safe::Canvas;
use std::ffi::CStr;
use std::sync::Arc;

struct Android;

impl presentation::Presentation for Android {
    fn surface_extension(&self) -> Option<&'static CStr> {
        Some(android_surface::NAME)
    }

    /// The swapchain extensions, plus the extensions needed to sample hardware buffers from other processes.
    fn device_extensions(&self) -> Vec<&'static CStr> {
        [
            presentation::SWAPCHAIN_EXTENSIONS.as_slice(),
            FOREIGN_IMPORT_EXTENSIONS.as_slice(),
            [hardware_buffer::NAME].as_slice(),
        ]
        .concat()
    }

    /// Sampler YCbCr conversion, through which hardware buffers in YCbCr formats are sampled.
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
}

/// An `ANativeWindow`, from which Vulkan creates a surface through `VK_KHR_android_surface`.
struct AndroidWindow(NativeWindow);

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new surface of `instance` for the window, and only the `Surface` that receives the surface
// destroys it. `NativeWindow` has a reference to the `ANativeWindow`, so the window exists as long as this value does.
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

impl VulkanRenderer {
    pub fn supports_presentation_time(&self) -> bool {
        self.device.contains(ash::google::display_timing::NAME)
    }

    /// Draws a frame whose buffer has the presentation time `presentation_time_ns`, in nanoseconds from 1 to
    /// `i64::MAX`. For a display Surface, the time is AOSP monotonic time. For an encoder Surface, the time is on the
    /// media timeline of the host. After this frame, every later frame of the Surface needs a presentation time too,
    /// see `SurfaceTiming`.
    ///
    /// # Errors
    /// Fails when the device lacks `VK_GOOGLE_display_timing`, the time is out of range, or the frame fails as
    /// `draw` does.
    pub fn draw_at(
        &mut self,
        target: &mut SurfaceTarget,
        size: (u32, u32),
        presentation_time_ns: u64,
        draw: impl FnOnce(&Canvas),
    ) -> Result<FrameOutcome, Error> {
        let time = wsi::PresentationTime::new(
            presentation_time_ns,
            self.device.contains(ash::google::display_timing::NAME),
        )?;
        self.draw_frame(target, size, (SurfaceId::UNSPECIFIED, 0), Some(time), draw)
    }

    /// A renderer that presents to AOSP windows and samples AOSP hardware buffers.
    ///
    /// # Errors
    /// Fails when no Vulkan device meets the requirements of the renderer, or when Vulkan or Skia cannot create the
    /// renderer.
    pub fn for_android() -> Result<Self, Error> {
        Self::create(&Android)
    }

    /// A target that presents to `window`, an `ANativeWindow` such as the Surface of a `SurfaceView` or the input
    /// Surface of an encoder. The target has a reference to the window, so the window is not released before the
    /// target. No other producer may use the window while the target exists. `timing` must be the timing history of
    /// this Surface across every attachment, see `SurfaceTiming`. The host must stop rendering and drop the target
    /// before the host reports the destruction of the Surface to AOSP.
    ///
    /// # Errors
    /// Fails when the renderer does not present to AOSP, or the device cannot present to the window.
    pub fn native_window_target(
        &self,
        window: NativeWindow,
        alpha: SurfaceAlpha,
        timing: SurfaceTiming,
    ) -> Result<SurfaceTarget, Error> {
        // The swapchain uses the identity transform, and the AOSP compositor rotates the buffer for the display.
        // Canvas and touch coordinates are therefore in the space of the host Surface.
        SurfaceTarget::new(
            self.device.clone(),
            Arc::new(AndroidWindow(window)),
            alpha,
            Some(vk::SurfaceTransformFlagsKHR::IDENTITY),
            timing,
        )
    }
}
