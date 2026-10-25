// SPDX-License-Identifier: GPL-3.0-only
use crate::fd::exported_fd;
use crate::sync::{Fence, SyncFdSemaphore};
use crate::{Device, Error, Semaphore, Status};
use ash::vk;
use std::os::fd::OwnedFd;
use std::sync::Arc;

/// Work submitted to the queue of the device, with the fence that the queue signals when the work is complete.
/// Dropping a `Submission` waits for that fence.
pub struct Submission {
    fence: Fence,
    _signaled: Option<SyncFdSemaphore>,
}

/// A submission that signals a sync fd, together with that fd. `fence` is `None` when the work was already complete at
/// the export, because Vulkan then returns -1 in place of a descriptor.
pub struct SyncFdSignal {
    pub submission: Submission,
    pub fence: Option<OwnedFd>,
}

impl Submission {
    pub(crate) fn new(fence: Fence) -> Self {
        Self {
            fence,
            _signaled: None,
        }
    }

    /// # Errors
    /// Fails when the device is lost.
    pub fn status(&self) -> Result<Status, Error> {
        self.fence.status()
    }

    /// # Errors
    /// Fails when the device is lost.
    pub fn wait(&self) -> Result<(), Error> {
        self.fence.wait()
    }
}

impl Drop for Submission {
    fn drop(&mut self) {
        if let Err(error) = self.fence.wait() {
            log::error!("Vulkan work is released before it completed: {error}");
        }
    }
}

impl Device {
    /// Submits an empty batch with a fence. The returned submission completes once all work submitted to the queue
    /// before it is complete.
    ///
    /// # Errors
    /// Fails when the driver cannot create a fence or submit.
    pub fn signal(self: &Arc<Self>) -> Result<Submission, Error> {
        let fence = Fence::new(self)?;
        self.submit(&vk::SubmitInfo::default(), &fence)?;
        Ok(Submission::new(fence))
    }

    /// Submits an empty batch that signals a semaphore and exports that signal as a sync fd. The fd signals once all
    /// work submitted to the queue before the batch is complete.
    ///
    /// # Errors
    /// Fails when the device lacks `VK_KHR_external_semaphore_fd`, or the driver cannot submit or export the fd.
    pub fn signal_sync_fd(self: &Arc<Self>) -> Result<SyncFdSignal, Error> {
        let semaphore = SyncFdSemaphore::new(self)?;
        let fence = Fence::new(self)?;
        let signals = [semaphore.handle()];
        self.submit(
            &vk::SubmitInfo::default().signal_semaphores(&signals),
            &fence,
        )?;
        let submission = Submission {
            fence,
            _signaled: Some(semaphore),
        };
        let functions =
            ash::khr::external_semaphore_fd::Device::new(self.instance().ash(), self.ash());
        let info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(signals[0])
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        // SAFETY: ash 0.38 has no safe form of vkGetSemaphoreFdKHR. The device has the extension enabled, as
        // `SyncFdSemaphore::new` required. The semaphore was created exportable as a sync fd. A sync fd export needs a
        // pending signal operation, and `submission` contains it.
        let raw = unsafe { functions.get_semaphore_fd(&info) }.map_err(Error::ExportSemaphore)?;
        // SAFETY: `raw` comes from the export above, and a sync fd export hands its descriptor to the caller alone.
        let fence = unsafe { exported_fd(raw) };
        Ok(SyncFdSignal { submission, fence })
    }

    /// Submits an empty batch that waits for `semaphore` in every pipeline stage. The wait consumes the pending signal
    /// of the semaphore.
    ///
    /// # Errors
    /// Fails when the semaphore belongs to another device, or the driver cannot create a fence or submit.
    ///
    /// # Safety
    /// A signal of `semaphore` must be pending or complete, and no other wait may consume it.
    pub unsafe fn wait_semaphore(
        self: &Arc<Self>,
        semaphore: &Semaphore,
    ) -> Result<Submission, Error> {
        self.same(semaphore.device())?;
        let fence = Fence::new(self)?;
        let waits = [semaphore.handle()];
        let stages = [vk::PipelineStageFlags::ALL_COMMANDS];
        let info = vk::SubmitInfo::default()
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&stages);
        self.submit(&info, &fence)?;
        Ok(Submission::new(fence))
    }

    fn submit(&self, info: &vk::SubmitInfo, fence: &Fence) -> Result<(), Error> {
        // SAFETY: ash 0.38 has no safe form of vkQueueSubmit. The lock in `with_queue` synchronizes the queue. The
        // fence is unsignaled and unused. The batch has no command buffers. A semaphore signaled by the batch exists
        // in the `Submission` until the fence signals. A semaphore waited for by the batch belongs to this device and
        // has a signal pending, as `wait_semaphore` requires of its caller. Dropping a `Semaphore` waits for the queue
        // to be idle before the semaphore is destroyed.
        self.with_queue(|queue| unsafe {
            self.ash()
                .queue_submit(queue, std::slice::from_ref(info), fence.handle())
        })
        .map_err(Error::Submit)
    }
}
