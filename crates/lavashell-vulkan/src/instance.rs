// SPDX-License-Identifier: GPL-3.0-only
use crate::ownership::Ownership;
use crate::{Error, ExtensionSet, Extensions, PhysicalDevice};
use ash::vk;
use std::any::Any;
use std::ffi::CStr;
use std::sync::Arc;

pub struct InstanceRequest<'a> {
    pub api_version: u32,
    pub extensions: &'a [&'static CStr],
}

/// A Vulkan instance. Dropping an instance made by [`Instance::create`] destroys it. An instance borrowed from a host
/// engine is destroyed by the host.
pub struct Instance {
    handle: Ownership<ash::Instance>,
    entry: ash::Entry,
    api_version: u32,
    extensions: Vec<&'static CStr>,
}

impl Instance {
    /// # Errors
    /// Fails when the implementation cannot list its extensions.
    pub fn available_extensions() -> Result<Extensions, Error> {
        let entry = ash::Entry::linked();
        // SAFETY: ash 0.38 has no safe form of vkEnumerateInstanceExtensionProperties. The linked entry is loaded
        // for the whole program, and no layer name is passed, so no pointer has to outlive the call.
        let properties = unsafe { entry.enumerate_instance_extension_properties(None) };
        properties
            .map(Extensions::new)
            .map_err(Error::InstanceExtensions)
    }

    /// # Errors
    /// Fails when a requested extension is missing or the implementation cannot create the instance.
    pub fn create(request: &InstanceRequest) -> Result<Arc<Self>, Error> {
        Self::available_extensions()?.require(request.extensions)?;
        let entry = ash::Entry::linked();
        let names = request
            .extensions
            .iter()
            .map(|name| name.as_ptr())
            .collect::<Vec<_>>();
        let application = vk::ApplicationInfo::default().api_version(request.api_version);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&application)
            .enabled_extension_names(&names);
        // SAFETY: ash 0.38 has no safe form of vkCreateInstance. `info`, the application info and the names pointed to
        // by `info` live through the call. `require` above found every name among the offered extensions.
        let instance =
            unsafe { entry.create_instance(&info, None) }.map_err(Error::CreateInstance)?;
        Ok(Arc::new(Self {
            handle: Ownership::Owned(instance),
            entry,
            api_version: request.api_version,
            extensions: request.extensions.to_vec(),
        }))
    }

    pub(crate) fn borrowed(
        entry: ash::Entry,
        instance: ash::Instance,
        api_version: u32,
        extensions: Vec<&'static CStr>,
        keep_alive: Arc<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            handle: Ownership::Borrowed {
                handle: instance,
                _keep_alive: keep_alive,
            },
            entry,
            api_version,
            extensions,
        }
    }

    /// # Errors
    /// Fails when the implementation cannot list its physical devices.
    pub fn physical_devices(self: &Arc<Self>) -> Result<Vec<PhysicalDevice<'_>>, Error> {
        // SAFETY: ash 0.38 has no safe form of vkEnumeratePhysicalDevices. Its one condition, a live instance,
        // is met while `self` is borrowed.
        let handles = unsafe { self.ash().enumerate_physical_devices() };
        let handles = handles.map_err(Error::PhysicalDevices)?;
        Ok(handles
            .into_iter()
            .map(|handle| PhysicalDevice::new(self, handle))
            .collect())
    }

    #[must_use]
    pub fn ash(&self) -> &ash::Instance {
        self.handle.handle()
    }

    #[must_use]
    pub fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    #[must_use]
    pub fn api_version(&self) -> u32 {
        self.api_version
    }

    #[must_use]
    pub fn extensions(&self) -> &[&'static CStr] {
        &self.extensions
    }
}

/// The extensions that the instance enables.
impl ExtensionSet for Instance {
    fn contains(&self, name: &CStr) -> bool {
        self.extensions.contains(&name)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Ownership::Owned(handle) = &self.handle {
            // SAFETY: ash 0.38 has no safe form of vkDestroyInstance. The instance was created here. Every device and
            // surface made from the instance has an `Arc` of the instance, so none of them is left when it drops.
            unsafe { handle.destroy_instance(None) };
        }
    }
}
