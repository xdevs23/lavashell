// SPDX-License-Identifier: GPL-3.0-only
use crate::{Device, Error, Instance};
use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;

/// A native window from which Vulkan creates a [`Surface`]. The surface contains an `Arc` of this value, because the
/// native window must exist as long as the surface does. The platform of the window implements this trait.
///
/// # Safety
/// `create_surface` must return a new surface of `instance` for the native window, which no other code destroys. The
/// native window must exist as long as this value does.
pub unsafe trait Window: Send + Sync {
    /// # Errors
    /// Fails when the instance lacks the surface extension needed by the window, when the native window is already
    /// destroyed, or when the driver cannot create the surface.
    fn create_surface(&self, instance: &Instance) -> Result<vk::SurfaceKHR, Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Supported,
    Unsupported,
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
