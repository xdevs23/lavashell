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
        // SAFETY: ash 0.38 has no safe form of vkDestroyFence. The fence was created on the live device, and a fence
        // is submitted only inside a `Submission`, which waits for it before the fence drops.
        unsafe { self.device.ash().destroy_fence(self.handle, None) };
    }
}

/// A binary semaphore. Dropping it waits until the queue is idle, because a submission made by another library may
/// still wait for the semaphore or signal it.
pub struct Semaphore {
    device: Arc<Device>,
    handle: vk::Semaphore,
}

impl Semaphore {
    /// # Errors
    /// Fails when the driver cannot create a semaphore.
    pub fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let handle = create(device, &vk::SemaphoreCreateInfo::default())?;
        Ok(Self {
            device: device.clone(),
            handle,
        })
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
        if let Err(error) = self.device.wait_queue_idle() {
            log::error!("a Vulkan semaphore is destroyed without finishing its work: {error}");
        }
        // SAFETY: ash 0.38 has no safe form of vkDestroySemaphore. The semaphore was created on the live device,
        // and the queue that every user of the semaphore submits to is idle, so no wait or signal of it is pending.
        unsafe { self.device.ash().destroy_semaphore(self.handle, None) };
    }
}

/// A semaphore created exportable as a sync fd. Its signal is exported once. The `Submission` that signals the
/// semaphore contains it, so the semaphore exists until that submission completes.
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
