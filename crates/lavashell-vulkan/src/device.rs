// SPDX-License-Identifier: GPL-3.0-only
use crate::ownership::Ownership;
use crate::{Error, ExtensionSet, Features, Instance, PhysicalDevice};
use ash::vk;
use std::any::Any;
use std::ffi::CStr;
use std::sync::{Arc, Mutex, PoisonError};

pub struct DeviceRequest<'a> {
    pub queue_family: u32,
    pub extensions: &'a [&'static CStr],
    pub features: Features,
}

/// The Vulkan handles of an instance and a device created by a host engine, for [`Device::borrow`]. The handles must
/// be valid as long as `keep_alive` exists.
pub struct HostDevice {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub instance_api_version: u32,
    pub instance_extensions: Vec<&'static CStr>,
    pub device: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub queue_family: u32,
    pub device_extensions: Vec<&'static CStr>,
    pub features: Features,
    pub keep_alive: Box<dyn Any + Send + Sync>,
}

/// A logical device with the one queue that every object of this crate uses.
pub struct Device {
    handle: Ownership<ash::Device>,
    queue: Mutex<vk::Queue>,
    queue_family: u32,
    physical: vk::PhysicalDevice,
    extensions: Vec<&'static CStr>,
    features: Features,
    instance: Arc<Instance>,
}

impl Device {
    /// # Errors
    /// Fails when the physical device lacks the queue family, an extension or a feature of `request`, or when the
    /// driver cannot create the device.
    pub fn create(
        physical: PhysicalDevice<'_>,
        request: &DeviceRequest,
    ) -> Result<Arc<Self>, Error> {
        let families = physical.queue_families().len();
        if request.queue_family as usize >= families {
            return Err(Error::QueueFamily(request.queue_family));
        }
        physical.extensions()?.require(request.extensions)?;
        let ycbcr_conversion = request.features.sampler_ycbcr_conversion;
        if ycbcr_conversion && !physical.features()?.sampler_ycbcr_conversion {
            return Err(Error::MissingYcbcrConversion);
        }
        let names = request
            .extensions
            .iter()
            .map(|name| name.as_ptr())
            .collect::<Vec<_>>();
        // A priority orders only the queues of one device against each other, so with one queue the value has no
        // effect.
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(request.queue_family)
            .queue_priorities(&priorities)];
        let mut ycbcr = vk::PhysicalDeviceSamplerYcbcrConversionFeatures::default()
            .sampler_ycbcr_conversion(true);
        let mut info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&names);
        if ycbcr_conversion {
            info = info.push_next(&mut ycbcr);
        }
        let instance = physical.instance();
        // SAFETY: ash 0.38 has no safe form of vkCreateDevice. The physical device belongs to the live instance, the
        // checks above found the queue family, the extensions and the features of `info` on it, and `info` with its
        // chain lives through the call.
        let device = unsafe { instance.ash().create_device(physical.handle(), &info, None) };
        let device = device.map_err(Error::CreateDevice)?;
        // SAFETY: ash 0.38 has no safe form of vkGetDeviceQueue. The device was just created with one queue of this
        // family, which is index 0.
        let queue = unsafe { device.get_device_queue(request.queue_family, 0) };
        Ok(Arc::new(Self {
            handle: Ownership::Owned(device),
            queue: Mutex::new(queue),
            queue_family: request.queue_family,
            physical: physical.handle(),
            extensions: request.extensions.to_vec(),
            features: request.features,
            instance: instance.clone(),
        }))
    }

    /// Wraps a device created by a host engine. This crate never destroys the borrowed device or its instance.
    ///
    /// # Safety
    /// The handles of `host` must be valid as long as `host.keep_alive` exists, and they must describe the instance,
    /// the device and its enabled extensions and features. The host must serialize every use of `host.queue` with
    /// the submissions and presentations made through the returned device.
    #[must_use]
    pub unsafe fn borrow(host: HostDevice) -> Arc<Self> {
        let keep_alive = Arc::from(host.keep_alive);
        let instance = Instance::borrowed(
            host.entry,
            host.instance,
            host.instance_api_version,
            host.instance_extensions,
            Arc::clone(&keep_alive),
        );
        Arc::new(Self {
            handle: Ownership::Borrowed {
                handle: host.device,
                _keep_alive: keep_alive,
            },
            queue: Mutex::new(host.queue),
            queue_family: host.queue_family,
            physical: host.physical,
            extensions: host.device_extensions,
            features: host.features,
            instance: Arc::new(instance),
        })
    }

    #[must_use]
    pub fn ash(&self) -> &ash::Device {
        self.handle.handle()
    }

    #[must_use]
    pub fn instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    #[must_use]
    pub fn physical_device(&self) -> PhysicalDevice<'_> {
        PhysicalDevice::new(&self.instance, self.physical)
    }

    #[must_use]
    pub fn queue_family(&self) -> u32 {
        self.queue_family
    }

    /// Returns the queue handle for a library, such as Skia, that submits work to the queue directly. The lock in this
    /// device does not order those submissions. The code that hands the handle to the library must order them with the
    /// submissions made through this device.
    #[must_use]
    pub fn queue_handle(&self) -> vk::Queue {
        self.with_queue(|queue| queue)
    }

    #[must_use]
    pub fn extensions(&self) -> &[&'static CStr] {
        &self.extensions
    }

    #[must_use]
    pub fn features(&self) -> Features {
        self.features
    }

    /// # Errors
    /// Fails when the device is lost.
    pub fn wait_idle(&self) -> Result<(), Error> {
        // SAFETY: ash 0.38 has no safe form of vkDeviceWaitIdle. The call needs every queue of the device synchronized
        // externally. The device has one queue, and the lock in `with_queue` synchronizes it.
        self.with_queue(|_| unsafe { self.ash().device_wait_idle() })
            .map_err(Error::WaitIdle)
    }

    pub(crate) fn wait_queue_idle(&self) -> Result<(), Error> {
        // SAFETY: ash 0.38 has no safe form of vkQueueWaitIdle. The call needs the queue synchronized externally, which
        // the lock in `with_queue` does.
        self.with_queue(|queue| unsafe { self.ash().queue_wait_idle(queue) })
            .map_err(Error::WaitIdle)
    }

    pub(crate) fn same(&self, other: &Self) -> Result<(), Error> {
        if std::ptr::eq(self, other) {
            Ok(())
        } else {
            Err(Error::ForeignObject)
        }
    }

    /// Runs `use_queue` while a lock prevents other threads from using the queue through this device. The lock does
    /// not cover submissions made by a host that lent the device, or made with the handle returned by `queue_handle`.
    /// The safety contracts of `Device::borrow` and of the unsafe calls that receive the handle require their callers
    /// to order those submissions with the ones made under the lock.
    pub(crate) fn with_queue<T>(&self, use_queue: impl FnOnce(vk::Queue) -> T) -> T {
        let queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        use_queue(*queue)
    }
}

/// The extensions that the device enables.
impl ExtensionSet for Device {
    fn contains(&self, name: &CStr) -> bool {
        self.extensions.contains(&name)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let Ownership::Owned(handle) = &self.handle else {
            return;
        };
        if let Err(error) = self.wait_idle() {
            log::error!("the Vulkan device is destroyed without finishing its work: {error}");
        }
        // SAFETY: ash 0.38 has no safe form of vkDestroyDevice. The device was created here. Every object made from the
        // device has an `Arc` of the device, so no such object is left when the device drops. The wait above either
        // finished all work on the queue or found the device lost, and a lost device runs no more work.
        unsafe { handle.destroy_device(None) };
    }
}
