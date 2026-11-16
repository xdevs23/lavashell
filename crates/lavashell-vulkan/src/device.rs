// SPDX-License-Identifier: GPL-3.0-only
use crate::ownership::{KeepAlive, Ownership};
use crate::{Error, ExtensionSet, Features, HostInstance, Instance, PhysicalDevice};
use ash::vk;
use lavashell_actor::lock_ignoring_poison;
use std::ffi::CStr;
use std::sync::{Arc, Mutex};

pub struct DeviceRequest<'a> {
    pub queue_family: u32,
    pub extensions: &'a [&'static CStr],
    pub features: Features,
}

pub struct HostDevice {
    pub instance: HostInstance,
    pub device: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub queue_family: u32,
    pub extensions: Vec<&'static CStr>,
    pub features: Features,
    pub keep_alive: KeepAlive,
}

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
    pub fn create(
        physical: PhysicalDevice<'_>,
        request: &DeviceRequest,
    ) -> Result<Arc<Self>, Error> {
        let families = physical.queue_families().len();
        if request.queue_family as usize >= families {
            return Err(Error::QueueFamily(request.queue_family));
        }
        physical.extensions()?.require(request.extensions)?;
        require_features(&physical, request.features)?;
        let names = request
            .extensions
            .iter()
            .map(|name| name.as_ptr())
            .collect::<Vec<_>>();
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(request.queue_family)
            .queue_priorities(&priorities)];
        let mut ycbcr = vk::PhysicalDeviceSamplerYcbcrConversionFeatures::default()
            .sampler_ycbcr_conversion(true);
        let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default()
            .swapchain_maintenance1(true);
        let mut info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&names);
        if request.features.sampler_ycbcr_conversion {
            info = info.push_next(&mut ycbcr);
        }
        if request.features.swapchain_maintenance1 {
            info = info.push_next(&mut maintenance);
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

    /// # Safety
    /// The handles of `host` must be valid as long as `host.keep_alive` exists, and they must describe the instance,
    /// the device and its enabled extensions and features. The host must serialize every use of `host.queue` with
    /// the submissions and presentations made through the returned device.
    #[must_use]
    pub unsafe fn borrow(host: HostDevice) -> Arc<Self> {
        let instance = Instance::borrowed(host.instance, Arc::clone(&host.keep_alive));
        Arc::new(Self {
            handle: Ownership::Borrowed {
                handle: host.device,
                _keep_alive: host.keep_alive,
            },
            queue: Mutex::new(host.queue),
            queue_family: host.queue_family,
            physical: host.physical,
            extensions: host.extensions,
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

    /// # Safety
    /// Every use of the returned queue must be serialized with the submissions and presentations made through this
    /// device and with every other use of the queue. The lock of this device protects only the uses made through it.
    #[must_use]
    pub unsafe fn queue_handle(&self) -> vk::Queue {
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

    pub fn wait_idle(&self) -> Result<(), Error> {
        // SAFETY: ash 0.38 has no safe form of vkDeviceWaitIdle. The call needs every queue of the device synchronized
        // externally. The device has one queue, and the lock in `with_queue` synchronizes it.
        self.with_queue(|_| unsafe { self.ash().device_wait_idle() })
            .map_err(Error::WaitIdle)
    }

    pub(crate) fn require_same_device(&self, other: &Self) -> Result<(), Error> {
        if std::ptr::eq(self, other) {
            Ok(())
        } else {
            Err(Error::ForeignObject)
        }
    }

    pub(crate) fn with_queue<T>(&self, use_queue: impl FnOnce(vk::Queue) -> T) -> T {
        let queue = lock_ignoring_poison(&self.queue);
        use_queue(*queue)
    }
}

fn require_features(physical: &PhysicalDevice<'_>, wanted: Features) -> Result<(), Error> {
    if wanted == Features::default() {
        return Ok(());
    }
    let offered = physical.features()?;
    if wanted.sampler_ycbcr_conversion && !offered.sampler_ycbcr_conversion {
        return Err(Error::MissingYcbcrConversion);
    }
    if wanted.swapchain_maintenance1 && !offered.swapchain_maintenance1 {
        return Err(Error::MissingSwapchainMaintenance);
    }
    Ok(())
}

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
