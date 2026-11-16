// SPDX-License-Identifier: GPL-3.0-only
use crate::{Device, Error, ExtensionSet};
use ash::vk;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Complete,
    Pending,
}

pub(crate) struct Fence {
    device: Arc<Device>,
    handle: vk::Fence,
}

impl Fence {
    pub(crate) fn new(device: &Arc<Device>) -> Result<Self, Error> {
        // SAFETY: ash 0.38 has no safe form of vkCreateFence. The device is live through its `Arc`, and the create
        // info has no chain.
        let handle = unsafe {
            device
                .ash()
                .create_fence(&vk::FenceCreateInfo::default(), None)
        };
        Ok(Self {
            device: device.clone(),
            handle: handle.map_err(Error::CreateFence)?,
        })
    }

    pub(crate) fn handle(&self) -> vk::Fence {
        self.handle
    }

    pub(crate) fn status(&self) -> Result<Status, Error> {
        // SAFETY: ash 0.38 has no safe form of vkGetFenceStatus. The fence was created on the device, of which this
        // fence has an `Arc`.
        let signaled = unsafe { self.device.ash().get_fence_status(self.handle) };
        if signaled.map_err(Error::FenceStatus)? {
            Ok(Status::Complete)
        } else {
            Ok(Status::Pending)
        }
    }

    pub(crate) fn wait(&self) -> Result<(), Error> {
        // SAFETY: ash 0.38 has no safe form of vkWaitForFences. The fence was created on the device, of which this
        // fence has an `Arc`. `u64::MAX` nanoseconds is the longest timeout Vulkan can express, so the wait ends only
        // when the fence signals or the device is lost.
        unsafe {
            self.device
                .ash()
                .wait_for_fences(&[self.handle], true, u64::MAX)
        }
        .map_err(Error::WaitFence)
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroyFence. The fence was created on the live device. A fence is
        // signaled only by a `Submission` or by a swapchain acquisition or presentation. A `Submission` waits for its
        // fence before the fence drops, and a swapchain drops a fence after it signaled or after the call that took
        // it failed.
        unsafe { self.device.ash().destroy_fence(self.handle, None) };
    }
}

pub struct Semaphore {
    device: Arc<Device>,
    handle: vk::Semaphore,
}

impl Semaphore {
    pub fn new(device: &Arc<Device>) -> Result<Arc<Self>, Error> {
        let handle = create(device, &vk::SemaphoreCreateInfo::default())?;
        Ok(Arc::new(Self {
            device: device.clone(),
            handle,
        }))
    }

    #[must_use]
    pub fn handle(&self) -> vk::Semaphore {
        self.handle
    }

    #[must_use]
    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroySemaphore. The semaphore was created on the live device. The
        // swapchain acquisition and presentation and `Device::wait_semaphore` hold an `Arc` of the semaphore until
        // their fence signals, and code that hands the handle to a queue keeps an `Arc` until that work completes. So
        // no wait or signal of the semaphore is pending.
        unsafe { self.device.ash().destroy_semaphore(self.handle, None) };
    }
}

pub(crate) struct SyncFdSemaphore {
    device: Arc<Device>,
    handle: vk::Semaphore,
}

impl SyncFdSemaphore {
    pub(crate) fn new(device: &Arc<Device>) -> Result<Self, Error> {
        device.require(&[ash::khr::external_semaphore_fd::NAME])?;
        let mut export = vk::ExportSemaphoreCreateInfo::default()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let handle = create(
            device,
            &vk::SemaphoreCreateInfo::default().push_next(&mut export),
        )?;
        Ok(Self {
            device: device.clone(),
            handle,
        })
    }

    pub(crate) fn handle(&self) -> vk::Semaphore {
        self.handle
    }
}

impl Drop for SyncFdSemaphore {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroySemaphore. The semaphore was created on the live device. The
        // `Submission` that contains the semaphore waits for its fence before the semaphore drops, and the queue
        // signals that fence after the semaphore.
        unsafe { self.device.ash().destroy_semaphore(self.handle, None) };
    }
}

fn create(device: &Device, info: &vk::SemaphoreCreateInfo) -> Result<vk::Semaphore, Error> {
    // SAFETY: ash 0.38 has no safe form of vkCreateSemaphore. The device is live while it is borrowed, and the chain
    // of `info` was built from typed structures of extensions the callers required.
    unsafe { device.ash().create_semaphore(info, None) }.map_err(Error::CreateSemaphore)
}
