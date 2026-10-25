// SPDX-License-Identifier: GPL-3.0-only
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WaylandWindowHandle,
    WindowHandle,
};
use sctk::reexports::client::Proxy;
use sctk::reexports::client::backend::Backend;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit as sctk;
use std::ptr::NonNull;

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

impl HasDisplayHandle for OwnedWlSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.backend.display_handle()
    }
}

impl HasWindowHandle for OwnedWlSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let proxy = NonNull::new(self.surface.id().as_ptr()).ok_or(HandleError::Unavailable)?;
        let handle = WaylandWindowHandle::new(proxy.cast());
        // SAFETY: raw-window-handle 0.6.2 builds a borrowed handle only through this unsafe constructor, and neither
        // wayland-client 0.31.15 nor smithay-client-toolkit 0.21.1 implements `HasWindowHandle` for a `wl_surface`,
        // so no safe form exists. The constructor needs a surface that is live for the lifetime of the borrow. The
        // pointer is non-null, so the proxy is not destroyed yet. Only the drop of this value destroys the proxy, and
        // that cannot happen while the handle borrows the value.
        Ok(unsafe { WindowHandle::borrow_raw(handle.into()) })
    }
}

impl Drop for OwnedWlSurface {
    fn drop(&mut self) {
        if self.surface.is_alive() {
            self.surface.destroy();
        }
    }
}
