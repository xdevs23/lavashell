// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, ExtensionSet, Extensions, Instance};
use ash::vk;
use std::sync::Arc;

/// A physical device of a live instance, usable only while the instance is borrowed.
#[derive(Clone, Copy)]
pub struct PhysicalDevice<'i> {
    instance: &'i Arc<Instance>,
    handle: vk::PhysicalDevice,
}

/// The optional device features used by this crate, as supported by a physical device or requested for a device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub sampler_ycbcr_conversion: bool,
}

/// A memory heap, with `budget`, the number of bytes of the heap that the process may allocate, and `usage`, the number
/// of bytes of the heap that the process uses at the moment of the query.
#[derive(Clone, Copy, Debug)]
pub struct HeapBudget {
    pub heap: vk::MemoryHeap,
    pub budget: vk::DeviceSize,
    pub usage: vk::DeviceSize,
}

impl<'i> PhysicalDevice<'i> {
    pub(crate) fn new(instance: &'i Arc<Instance>, handle: vk::PhysicalDevice) -> Self {
        Self { instance, handle }
    }

    #[must_use]
    pub fn handle(&self) -> vk::PhysicalDevice {
        self.handle
    }

    #[must_use]
    pub fn instance(&self) -> &'i Arc<Instance> {
        self.instance
    }

    pub fn properties(&self) -> vk::PhysicalDeviceProperties {
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceProperties. The handle belongs to the instance,
        // which is live while it is borrowed.
        unsafe {
            self.instance
                .ash()
                .get_physical_device_properties(self.handle)
        }
    }

    /// # Errors
    /// Fails when the driver reports a name without a terminating zero or in another encoding than UTF-8.
    pub fn name(&self) -> Result<String, Error> {
        let properties = self.properties();
        let name = properties
            .device_name_as_c_str()
            .map_err(Error::DeviceName)?;
        let name = name.to_str().map_err(Error::DeviceNameEncoding)?;
        Ok(name.to_owned())
    }

    /// # Errors
    /// Fails when the instance or the device is older than Vulkan 1.1.
    pub fn device_uuid(&self) -> Result<[u8; vk::UUID_SIZE], Error> {
        let mut id = vk::PhysicalDeviceIDProperties::default();
        self.read_properties(&mut vk::PhysicalDeviceProperties2::default().push_next(&mut id))?;
        Ok(id.device_uuid)
    }

    fn read_properties(&self, properties: &mut vk::PhysicalDeviceProperties2) -> Result<(), Error> {
        self.require_version(vk::API_VERSION_1_1)?;
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceProperties2. The handle belongs to the borrowed,
        // live instance. The version check above confirmed Vulkan 1.1, which the call needs. The callers chain only
        // structures of the Vulkan versions that they checked.
        unsafe {
            self.instance
                .ash()
                .get_physical_device_properties2(self.handle, properties);
        }
        Ok(())
    }

    /// # Errors
    /// Fails when the instance or the device is older than Vulkan 1.1.
    pub fn features(&self) -> Result<Features, Error> {
        self.require_version(vk::API_VERSION_1_1)?;
        let mut ycbcr = vk::PhysicalDeviceSamplerYcbcrConversionFeatures::default();
        let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut ycbcr);
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceFeatures2. The handle belongs to the borrowed,
        // live instance. The version check above confirmed Vulkan 1.1, which the call and its chain need.
        unsafe {
            self.instance
                .ash()
                .get_physical_device_features2(self.handle, &mut features);
        }
        Ok(Features {
            sampler_ycbcr_conversion: ycbcr.sampler_ycbcr_conversion == vk::TRUE,
        })
    }

    /// # Errors
    /// Fails when the driver cannot list the extensions.
    pub fn extensions(&self) -> Result<Extensions, Error> {
        // SAFETY: ash 0.38 has no safe form of vkEnumerateDeviceExtensionProperties. The handle belongs to the
        // borrowed, live instance, and no layer name is passed.
        let properties = unsafe {
            self.instance
                .ash()
                .enumerate_device_extension_properties(self.handle)
        };
        properties
            .map(Extensions::new)
            .map_err(Error::DeviceExtensions)
    }

    #[must_use]
    pub fn queue_families(&self) -> Vec<vk::QueueFamilyProperties> {
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceQueueFamilyProperties. The handle belongs
        // to the instance, which is live while it is borrowed.
        unsafe { (self.instance.ash()).get_physical_device_queue_family_properties(self.handle) }
    }

    pub fn memory_properties(&self) -> vk::PhysicalDeviceMemoryProperties {
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceMemoryProperties. The handle belongs to
        // the instance, which is live while it is borrowed.
        unsafe {
            self.instance
                .ash()
                .get_physical_device_memory_properties(self.handle)
        }
    }

    /// The budget and the usage of every memory heap, or `None` when the device lacks Vulkan 1.1 or
    /// `VK_EXT_memory_budget`.
    ///
    /// # Errors
    /// Fails when the driver cannot list the extensions of the device.
    pub fn memory_budget(&self) -> Result<Option<Vec<HeapBudget>>, Error> {
        if self.version() < vk::API_VERSION_1_1
            || !self.extensions()?.contains(ash::ext::memory_budget::NAME)
        {
            return Ok(None);
        }
        let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
        let mut properties = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
        // SAFETY: ash 0.38 has no safe form of vkGetPhysicalDeviceMemoryProperties2. The handle belongs to the
        // borrowed, live instance. The version check above confirmed Vulkan 1.1, which the call needs. The device
        // offers `VK_EXT_memory_budget`, which defines the structure in the chain. The chain lives through the call.
        unsafe {
            self.instance
                .ash()
                .get_physical_device_memory_properties2(self.handle, &mut properties);
        }
        let memory = properties.memory_properties;
        let heaps = (memory.memory_heaps_as_slice().iter())
            .zip(budget.heap_budget.into_iter().zip(budget.heap_usage))
            .map(|(heap, (budget, usage))| HeapBudget {
                heap: *heap,
                budget,
                usage,
            });
        Ok(Some(heaps.collect()))
    }

    pub(crate) fn require_version(&self, required: u32) -> Result<(), Error> {
        let available = self.version();
        if available < required {
            return Err(Error::ApiVersion {
                required,
                available,
            });
        }
        Ok(())
    }

    /// The Vulkan version that both the instance and the device support.
    fn version(&self) -> u32 {
        self.instance
            .api_version()
            .min(self.properties().api_version)
    }
}
