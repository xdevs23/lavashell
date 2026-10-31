// SPDX-License-Identifier: GPL-3.0-only
use crate::wayland_presentation::wayland_functions;
use ash::vk;
use lavashell_vulkan::{Instance, Window};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::backend::Backend;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit as sctk;

/// The `wl_surface` of a native surface. Only the drop of this value destroys the `wl_surface`.
pub(super) struct OwnedWlSurface {
    surface: WlSurface,
    backend: Backend,
}

impl OwnedWlSurface {
    pub(super) fn new(surface: WlSurface, backend: Backend) -> Self {
        Self { surface, backend }
    }
}

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new surface of `instance` for the `wl_surface`, and only the `Surface` that receives the
// surface destroys it. Only the drop of this value destroys the `wl_surface`, and the `wl_display` is connected as long
// as the backend exists, so both exist as long as this value does.
unsafe impl Window for OwnedWlSurface {
    fn create_surface(
        &self,
        instance: &Instance,
    ) -> Result<vk::SurfaceKHR, lavashell_vulkan::Error> {
        let functions = wayland_functions(instance)?;
        let proxy = self.surface.id().as_ptr();
        if proxy.is_null() {
            return Err(lavashell_vulkan::Error::DestroyedWindow);
        }
        let info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(self.backend.display_ptr().cast())
            .surface(proxy.cast());
        // SAFETY: ash 0.38 has no safe form of vkCreateWaylandSurfaceKHR, and the maintained crates cannot make a
        // surface from a `wl_surface` safely. The instance has the extension enabled, as `wayland_functions` checked.
        // The display pointer is the live `wl_display` of the backend, and the proxy pointer is non-null, so the
        // `wl_surface` is not destroyed yet. The `Surface` contains an `Arc` of this value, so both outlive the
        // Vulkan surface.
        unsafe { functions.create_wayland_surface(&info, None) }
            .map_err(lavashell_vulkan::Error::CreateSurface)
    }
}

impl Drop for OwnedWlSurface {
    fn drop(&mut self) {
        if self.surface.is_alive() {
            self.surface.destroy();
        }
    }
}
