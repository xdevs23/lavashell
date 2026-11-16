// SPDX-License-Identifier: GPL-3.0-only
use crate::{Device, Error, Instance};
use ash::vk;
use std::ffi::CStr;
use std::sync::Arc;

/// # Safety
/// `create_surface` must return a new surface of `instance` for the native window, which no other code destroys. The
/// native window must exist as long as this value does.
pub unsafe trait Window: Send + Sync {
    fn create_surface(&self, instance: &Instance) -> Result<vk::SurfaceKHR, Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Supported,
    Unsupported,
}

pub struct Surface {
    handle: vk::SurfaceKHR,
    functions: ash::khr::surface::Instance,
    instance: Arc<Instance>,
    _window: Arc<dyn Window>,
}

impl Surface {
    pub fn new(instance: &Arc<Instance>, window: Arc<dyn Window>) -> Result<Arc<Self>, Error> {
        let handle = window.create_surface(instance)?;
        Ok(Arc::new(Self {
            handle,
            functions: ash::khr::surface::Instance::new(instance.entry(), instance.ash()),
            instance: instance.clone(),
            _window: window,
        }))
    }

    #[must_use]
    pub fn instance_extensions(surface_extension: &'static CStr) -> [&'static CStr; 4] {
        [
            ash::khr::surface::NAME,
            ash::khr::get_surface_capabilities2::NAME,
            ash::ext::surface_maintenance1::NAME,
            surface_extension,
        ]
    }

    pub(crate) fn handle(&self) -> vk::SurfaceKHR {
        self.handle
    }

    pub fn presentation(&self, device: &Device) -> Result<Presentation, Error> {
        let physical = self.physical_handle_of(device)?;
        let family = device.queue_family();
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceSupportKHR. The physical device and the
        // surface belong to the same live instance, which `physical_handle_of` checked. `family` is the queue family
        // of the device.
        let supported = unsafe {
            self.functions
                .get_physical_device_surface_support(physical, family, self.handle)
        };
        if supported.map_err(Error::SurfaceSupport)? {
            Ok(Presentation::Supported)
        } else {
            Ok(Presentation::Unsupported)
        }
    }

    pub fn capabilities(&self, device: &Device) -> Result<vk::SurfaceCapabilitiesKHR, Error> {
        let physical = self.physical_handle_of(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceCapabilitiesKHR. The physical device and
        // the surface belong to the same live instance, which `physical_handle_of` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_capabilities(physical, self.handle)
        }
        .map_err(Error::SurfaceCapabilities)
    }

    pub fn formats(&self, device: &Device) -> Result<Vec<vk::SurfaceFormatKHR>, Error> {
        let physical = self.physical_handle_of(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfaceFormatsKHR. The physical device and the
        // surface belong to the same live instance, which `physical_handle_of` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_formats(physical, self.handle)
        }
        .map_err(Error::SurfaceFormats)
    }

    pub fn present_modes(&self, device: &Device) -> Result<Vec<vk::PresentModeKHR>, Error> {
        let physical = self.physical_handle_of(device)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceSurfacePresentModesKHR. The physical device and
        // the surface belong to the same live instance, which `physical_handle_of` checked.
        unsafe {
            self.functions
                .get_physical_device_surface_present_modes(physical, self.handle)
        }
        .map_err(Error::SurfacePresentModes)
    }

    fn physical_handle_of(&self, device: &Device) -> Result<vk::PhysicalDevice, Error> {
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
