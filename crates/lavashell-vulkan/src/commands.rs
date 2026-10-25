// SPDX-License-Identifier: GPL-3.0-only
mod transition;

use crate::sync::Fence;
use crate::{Device, Error, Image, Status, Submission};
use ash::vk;
use std::sync::Arc;
pub use transition::{ImageState, QueueFamily};

/// A primary command buffer in a pool of its own, ready to record.
pub struct Commands {
    buffer: vk::CommandBuffer,
    pool: CommandPool,
}

struct CommandPool {
    device: Arc<Device>,
    handle: vk::CommandPool,
}

/// A command buffer being recorded, with the layout and queue family of every image used by the recorded commands,
/// as tracked by the recording.
pub struct Recording {
    commands: Commands,
    images: Vec<(Arc<Image>, ImageState)>,
}

/// A command buffer whose recording is finished, with references to the images used by its commands.
pub struct Recorded {
    commands: Commands,
    _images: Vec<Arc<Image>>,
}

/// A recorded command buffer submitted to the queue. Dropping a `Submitted` blocks until its commands are complete.
pub struct Submitted {
    submission: Submission,
    recorded: Recorded,
}

impl Commands {
    /// # Errors
    /// Fails when the driver cannot create the pool or allocate the buffer.
    pub fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let info = vk::CommandPoolCreateInfo::default().queue_family_index(device.queue_family());
        // SAFETY: ash 0.38 has no safe form of vkCreateCommandPool. The device exists as long as its `Arc`, and the
        // queue of the device belongs to the family set in `info`.
        let handle = unsafe { device.ash().create_command_pool(&info, None) }
            .map_err(Error::CreateCommandPool)?;
        let pool = CommandPool {
            device: device.clone(),
            handle,
        };
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: ash 0.38 has no safe form of vkAllocateCommandBuffers. The pool was created on the live device
        // just above, and nothing else uses it yet.
        let buffers = unsafe { device.ash().allocate_command_buffers(&info) }
            .map_err(Error::AllocateCommandBuffer)?;
        let buffer = buffers.first().copied().ok_or(Error::NoCommandBuffer)?;
        Ok(Self { buffer, pool })
    }

    /// # Errors
    /// Fails when the driver cannot reset the pool or begin recording.
    pub fn record(self) -> Result<Recording, Error> {
        let device = &self.pool.device;
        // SAFETY: ash 0.38 has no safe form of vkResetCommandPool. The pool belongs to the live device, and only `self`
        // uses it. A `Commands` value exists only while no submission of its buffer is pending.
        unsafe {
            device
                .ash()
                .reset_command_pool(self.pool.handle, vk::CommandPoolResetFlags::empty())
        }
        .map_err(Error::ResetCommandPool)?;
        let info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: ash 0.38 has no safe form of vkBeginCommandBuffer. The buffer was reset with its pool, and only
        // `self` uses that pool.
        unsafe { device.ash().begin_command_buffer(self.buffer, &info) }
            .map_err(Error::BeginCommands)?;
        Ok(Recording {
            commands: self,
            images: Vec::new(),
        })
    }

    #[must_use]
    pub fn device(&self) -> &Arc<Device> {
        &self.pool.device
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroyCommandPool. The pool belongs to the live device. A submitted
        // buffer from this pool is inside a `Submitted`, which waits for the submission before the pool is dropped.
        unsafe { self.device.ash().destroy_command_pool(self.handle, None) };
    }
}

impl Recording {
    /// Moves `image` into `layout`, discarding its contents.
    ///
    /// # Errors
    /// Fails when the image belongs to another device.
    pub fn discard(&mut self, image: &Arc<Image>, layout: vk::ImageLayout) -> Result<(), Error> {
        let from = ImageState::new(vk::ImageLayout::UNDEFINED, QueueFamily::Device);
        self.track(image, from)?;
        self.transition(image, ImageState::new(layout, QueueFamily::Device))
    }

    /// Declares the state of `image` when these commands start.
    ///
    /// # Errors
    /// Fails when the image belongs to another device.
    ///
    /// # Safety
    /// When the commands run, `image` must be in `state`, and every earlier write to the image must be available.
    /// When the queue family of the image changes, the previous queue family must have released the image with the
    /// same layouts.
    pub unsafe fn assume(&mut self, image: &Arc<Image>, state: ImageState) -> Result<(), Error> {
        self.track(image, state)
    }

    /// Records a barrier that moves `image` into the state `to`. Every write before the barrier is visible to every
    /// access after it.
    ///
    /// # Errors
    /// Fails when the recording does not know the state of the image.
    pub fn transition(&mut self, image: &Arc<Image>, to: ImageState) -> Result<(), Error> {
        let from = self.state(image)?;
        let family = self.commands.device().queue_family();
        let barriers = transition::barriers(from, to, family)?
            .into_iter()
            .map(|barrier| {
                barrier
                    .image(image.handle())
                    .subresource_range(Image::subresources())
            })
            .collect::<Vec<_>>();
        if !barriers.is_empty() {
            let stages = vk::PipelineStageFlags::ALL_COMMANDS;
            let device = self.commands.device().ash();
            let buffer = self.commands.buffer;
            // SAFETY: ash 0.38 has no safe form of vkCmdPipelineBarrier. The buffer is recording. The image belongs
            // to the device of the buffer and exists as long as these commands. The old states are the states
            // tracked by this recording.
            unsafe {
                device.cmd_pipeline_barrier(
                    buffer,
                    stages,
                    stages,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            };
        }
        self.set_state(image, to);
        Ok(())
    }

    /// Records a clear of every pixel of `image` to `color`.
    ///
    /// # Errors
    /// Fails when the recording does not know the state of the image, or the image is neither in
    /// `TRANSFER_DST_OPTIMAL` nor in `GENERAL` on this queue.
    pub fn clear(&mut self, image: &Arc<Image>, color: vk::ClearColorValue) -> Result<(), Error> {
        let state = self.state(image)?;
        let writable = [
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        ];
        if state.family != QueueFamily::Device || !writable.contains(&state.layout) {
            return Err(Error::ClearLayout(state.layout));
        }
        let device = self.commands.device().ash();
        let buffer = self.commands.buffer;
        let ranges = [Image::subresources()];
        // SAFETY: ash 0.38 has no safe form of vkCmdClearColorImage. The buffer is recording. The image belongs to the
        // device of the buffer, exists as long as these commands, and is in a layout that allows a clear.
        unsafe {
            device.cmd_clear_color_image(buffer, image.handle(), state.layout, &color, &ranges);
        }
        Ok(())
    }

    /// # Errors
    /// Fails when the driver cannot finish the recording.
    pub fn finish(self) -> Result<Recorded, Error> {
        let device = self.commands.device().ash();
        // SAFETY: ash 0.38 has no safe form of vkEndCommandBuffer. The buffer is recording, and only `self` uses its
        // pool.
        unsafe { device.end_command_buffer(self.commands.buffer) }.map_err(Error::EndCommands)?;
        Ok(Recorded {
            commands: self.commands,
            _images: self.images.into_iter().map(|(image, _)| image).collect(),
        })
    }

    pub(crate) fn buffer(&self) -> vk::CommandBuffer {
        self.commands.buffer
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        self.commands.device()
    }

    fn track(&mut self, image: &Arc<Image>, state: ImageState) -> Result<(), Error> {
        self.commands.device().same(image.device())?;
        self.set_state(image, state);
        Ok(())
    }

    fn state(&self, image: &Arc<Image>) -> Result<ImageState, Error> {
        let known = self
            .images
            .iter()
            .find(|(known, _)| Arc::ptr_eq(known, image));
        known
            .map(|(_, state)| *state)
            .ok_or(Error::UnknownImageState)
    }

    fn set_state(&mut self, image: &Arc<Image>, state: ImageState) {
        match self
            .images
            .iter_mut()
            .find(|(known, _)| Arc::ptr_eq(known, image))
        {
            Some((_, known)) => *known = state,
            None => self.images.push((image.clone(), state)),
        }
    }
}

impl Recorded {
    /// # Errors
    /// Fails when the driver cannot create a fence or submit.
    pub fn submit(self) -> Result<Submitted, Error> {
        let device = self.commands.device();
        let fence = Fence::new(device)?;
        let buffers = [self.commands.buffer];
        let info = vk::SubmitInfo::default().command_buffers(&buffers);
        // SAFETY: ash 0.38 has no safe form of vkQueueSubmit. `with_queue` synchronizes the access to the queue. The
        // fence is new and unsignaled, and the commands were recorded completely on this device. The returned
        // `Submitted` has references to the pool and to the images used by the commands, and it waits for the fence
        // before it is dropped.
        let submitted = device.with_queue(|queue| unsafe {
            device.ash().queue_submit(queue, &[info], fence.handle())
        });
        submitted.map_err(Error::Submit)?;
        Ok(Submitted {
            submission: Submission::new(fence),
            recorded: self,
        })
    }
}

impl Submitted {
    /// # Errors
    /// Fails when the device is lost.
    pub fn status(&self) -> Result<Status, Error> {
        self.submission.status()
    }

    /// Waits until the submission is complete and returns the command buffer, ready to record again.
    ///
    /// # Errors
    /// Fails when the device is lost.
    pub fn reclaim(self) -> Result<Commands, Error> {
        self.submission.wait()?;
        Ok(self.recorded.commands)
    }
}
