// SPDX-License-Identifier: GPL-3.0-only
use crate::{Device, Error, ExtensionSet, Instance, PhysicalDevice};
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::ffi::{CStr, c_void};
use std::ptr::NonNull;
use std::sync::Arc;

/// A native window from which Vulkan creates a [`Surface`]. The surface contains an `Arc` of this value, because the
/// native window must exist as long as the surface does. A window with the raw handles of a [`WindowSystem`] has this
/// trait already, and the platform of any other window implements it.
///
/// # Safety
/// `create_surface` must return a new surface of `instance` for the native window, which no other code destroys. The
/// native window must exist as long as this value does.
pub unsafe trait Window: Send + Sync {
    /// # Errors
    /// Fails when the instance lacks the surface extension of the window, when the window offers no handle of a window
    /// system that the implementation knows, or when the driver cannot create the surface.
    fn create_surface(&self, instance: &Instance) -> Result<vk::SurfaceKHR, Error>;
}

// SAFETY: A `Surface` destroys the handle that `create_surface` returns and cannot check it, so the trait is unsafe.
// `create_surface` creates a new surface of `instance` from the raw handles of the window, and only the `Surface` that
// receives the surface destroys it. The handles point to live objects of the window system while the window exists,
// as `HasWindowHandle` and `HasDisplayHandle` require of the window.
unsafe impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + ?Sized> Window for T {
    fn create_surface(&self, instance: &Instance) -> Result<vk::SurfaceKHR, Error> {
        let window = self.window_handle().map_err(Error::WindowHandle)?.as_raw();
        Display::of(self)?.create_surface(instance, window)
    }
}

/// A window system from whose raw window handles this crate creates surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSystem {
    Wayland,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Supported,
    Unsupported,
}

/// The display connection of a window system, with the `wl_display` pointer that Vulkan needs for Wayland.
#[derive(Clone, Copy)]
enum Display {
    Wayland(NonNull<c_void>),
}

impl WindowSystem {
    /// # Errors
    /// Fails when the display offers no handle or belongs to another window system.
    pub fn of(display: &(impl HasDisplayHandle + ?Sized)) -> Result<Self, Error> {
        Display::of(display).map(Display::system)
    }

    /// The instance extension that creates the surfaces of this window system.
    #[must_use]
    pub fn surface_extension(self) -> &'static CStr {
        match self {
            Self::Wayland => ash::khr::wayland_surface::NAME,
        }
    }
}

impl Display {
    fn of(display: &(impl HasDisplayHandle + ?Sized)) -> Result<Self, Error> {
        match display
            .display_handle()
            .map_err(Error::WindowHandle)?
            .as_raw()
        {
            RawDisplayHandle::Wayland(handle) => Ok(Self::Wayland(handle.display)),
            _ => Err(Error::UnsupportedWindowSystem),
        }
    }

    fn system(self) -> WindowSystem {
        match self {
            Self::Wayland(_) => WindowSystem::Wayland,
        }
    }

    /// Whether queue family `family` of `physical` can present to this display. The caller must borrow the display
    /// for the duration of the call.
    fn presents(self, physical: &PhysicalDevice<'_>, family: u32) -> Result<Presentation, Error> {
        let instance = physical.instance();
        instance.require(&[self.system().surface_extension()])?;
        let Self::Wayland(display) = self;
        let functions = ash::khr::wayland_surface::Instance::new(instance.entry(), instance.ash());
        let query = functions
            .fp()
            .get_physical_device_wayland_presentation_support_khr;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceWaylandPresentationSupportKHR. The ash wrapper also
        // requires a `&mut wl_display`, which cannot be made from a display shared with libwayland, so this code calls
        // the function pointer with the raw pointer. The physical device belongs to the live instance, which has
        // Wayland surfaces enabled. The handle points to a live `wl_display` while the caller borrows the display.
        let supported = unsafe { query(physical.handle(), family, display.as_ptr().cast()) };
        Ok(if supported == vk::TRUE {
            Presentation::Supported
        } else {
            Presentation::Unsupported
        })
    }

    /// Creates a Vulkan surface for `window` on this display. The window must be live as long as the surface exists.
    fn create_surface(
        self,
        instance: &Instance,
        window: RawWindowHandle,
    ) -> Result<vk::SurfaceKHR, Error> {
        instance.require(&[self.system().surface_extension()])?;
        let (Self::Wayland(display), RawWindowHandle::Wayland(surface)) = (self, window) else {
            return Err(Error::UnsupportedWindowSystem);
        };
        let functions = ash::khr::wayland_surface::Instance::new(instance.entry(), instance.ash());
        let info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(display.as_ptr())
            .surface(surface.surface.as_ptr());
        // SAFETY: ash 0.38 has no safe form of vkCreateWaylandSurfaceKHR, and no maintained crate makes a surface from
        // window handles safely. The instance has the extension enabled, as `require` checked. Both handles point to
        // live Wayland objects while the window exists. The `Surface` contains an `Arc` of the window, so the window
        // exists as long as the surface.
        unsafe { functions.create_wayland_surface(&info, None) }.map_err(Error::CreateSurface)
    }
}

impl PhysicalDevice<'_> {
    /// # Errors
    /// Fails when the display offers no handle, or when the instance lacks the surface extension of the window system
    /// of the display.
    pub fn presents(
        &self,
        family: u32,
        display: &(impl HasDisplayHandle + ?Sized),
    ) -> Result<Presentation, Error> {
        Display::of(display)?.presents(self, family)
    }
}

/// A Vulkan surface for a window. The surface contains an `Arc` of the window, so the window drops only after the
/// surface is destroyed.
pub struct Surface {
    handle: vk::SurfaceKHR,
    functions: ash::khr::surface::Instance,
    instance: Arc<Instance>,
    _window: Arc<dyn Window>,
}

impl Surface {
    /// # Errors
    /// Fails as [`Window::create_surface`] does.
    pub fn new(instance: &Arc<Instance>, window: Arc<dyn Window>) -> Result<Arc<Self>, Error> {
        let handle = window.create_surface(instance)?;
        Ok(Arc::new(Self {
            handle,
            functions: ash::khr::surface::Instance::new(instance.entry(), instance.ash()),
            instance: instance.clone(),
            _window: window,
        }))
    }

    /// The instance extensions that a surface needs: `VK_KHR_surface` and `surface_extension`, the extension that
    /// creates the surfaces of a window system.
    #[must_use]
    pub fn instance_extensions(surface_extension: &'static CStr) -> [&'static CStr; 2] {
        [ash::khr::surface::NAME, surface_extension]
    }

    #[must_use]
    pub fn handle(&self) -> vk::SurfaceKHR {
        self.handle
    }

    /// # Errors
    /// Fails when the device belongs to another instance, or when the query fails in the driver.
    pub fn presentation(&self, device: &Device) -> Result<Presentation, Error> {
        let physical = self.physical(device)?;
        let family = device.queue_family();
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceSupportKHR. The physical device and the
        // surface belong to the same live instance, which `physical` checked. `family` is the queue family of the
        // device.
        let supported = unsafe {
            self.functions
                .get_physical_device_surface_support(physical, family, self.handle)
        };
        if supported.map_err(Error::SurfaceQuery)? {
            Ok(Presentation::Supported)
        } else {
            Ok(Presentation::Unsupported)
        }
    }

    /// # Errors
    /// Fails when the device belongs to another instance, or when the query fails in the driver.
    pub fn capabilities(&self, device: &Device) -> Result<vk::SurfaceCapabilitiesKHR, Error> {
        let physical = self.physical(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceCapabilitiesKHR. The physical device and
        // the surface belong to the same live instance, which `physical` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_capabilities(physical, self.handle)
        }
        .map_err(Error::SurfaceQuery)
    }

    /// # Errors
    /// Fails when the device belongs to another instance, or when the query fails in the driver.
    pub fn formats(&self, device: &Device) -> Result<Vec<vk::SurfaceFormatKHR>, Error> {
        let physical = self.physical(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceFormatsKHR. The physical device and the
        // surface belong to the same live instance, which `physical` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_formats(physical, self.handle)
        }
        .map_err(Error::SurfaceQuery)
    }

    /// # Errors
    /// Fails when the device belongs to another instance, or when the query fails in the driver.
    pub fn present_modes(&self, device: &Device) -> Result<Vec<vk::PresentModeKHR>, Error> {
        let physical = self.physical(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfacePresentModesKHR. The physical device and
        // the surface belong to the same live instance, which `physical` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_present_modes(physical, self.handle)
        }
        .map_err(Error::SurfaceQuery)
    }

    fn physical(&self, device: &Device) -> Result<vk::PhysicalDevice, Error> {
        if Arc::ptr_eq(device.instance(), &self.instance) {
            Ok(device.physical_device().handle())
        } else {
            Err(Error::ForeignObject)
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroySurfaceKHR. The surface belongs to the instance of which this
        // surface has an `Arc`. Every swapchain created for the surface has an `Arc` of the surface, so no swapchain
        // is left.
        unsafe { self.functions.destroy_surface(self.handle, None) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::DisplayHandle;

    #[test]
    fn a_display_of_another_window_system_is_refused() {
        assert!(matches!(
            WindowSystem::of(&DisplayHandle::web()),
            Err(Error::UnsupportedWindowSystem)
        ));
    }
}
