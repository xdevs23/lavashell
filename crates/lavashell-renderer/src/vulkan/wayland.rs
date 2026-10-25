// SPDX-License-Identifier: GPL-3.0-only
use super::{
    PresentationPolicy, SurfaceAlpha, SurfaceTarget, SurfaceTiming, VulkanRenderer, presentation,
};
use crate::Error;
use lavashell_vulkan::{Features, PhysicalDevice, WindowSystem};
use raw_window_handle::{DisplayHandle, HasDisplayHandle, HasWindowHandle, RawDisplayHandle};
use std::ffi::CStr;
use std::sync::Arc;

pub type WaylandTarget = SurfaceTarget;

struct Wayland<'a>(DisplayHandle<'a>);

impl presentation::Presentation for Wayland<'_> {
    fn surface_extension(&self) -> Option<&'static CStr> {
        Some(WindowSystem::Wayland.surface_extension())
    }

    fn device_extensions(&self) -> Vec<&'static CStr> {
        presentation::SWAPCHAIN_EXTENSIONS.to_vec()
    }

    fn features(&self) -> Features {
        Features::default()
    }

    fn supports(
        &self,
        physical: &PhysicalDevice<'_>,
        family: u32,
    ) -> Result<lavashell_vulkan::Presentation, lavashell_vulkan::Error> {
        physical.presents(family, &self.0)
    }
}

impl VulkanRenderer {
    /// A renderer whose device presents to windows of the Wayland `display`.
    ///
    /// # Errors
    /// Fails when `display` has no Wayland display handle, when no Vulkan device meets the requirements of the
    /// renderer, or when Vulkan or Skia cannot create the renderer.
    pub fn for_wayland(display: &impl HasDisplayHandle) -> Result<Self, Error> {
        let display = display.display_handle().map_err(Error::DisplayHandle)?;
        if !matches!(display.as_raw(), RawDisplayHandle::Wayland(_)) {
            return Err(Error::DisplayNotWayland);
        }
        Self::create(&Wayland(display))
    }

    /// A target that presents to `window`, a Wayland surface on the display of this renderer. The target has a
    /// reference to `window`, so the window is not dropped before the target.
    ///
    /// # Errors
    /// Fails when the renderer does not present to Wayland, when `window` has no Wayland window handle, or when the
    /// device cannot present to the surface of `window`.
    pub fn wayland_target(
        &self,
        window: Arc<impl HasWindowHandle + HasDisplayHandle + Send + Sync + 'static>,
        alpha: SurfaceAlpha,
    ) -> Result<WaylandTarget, Error> {
        SurfaceTarget::new(
            self.device.clone(),
            window,
            alpha,
            None,
            SurfaceTiming::new(PresentationPolicy::Automatic),
        )
    }
}
