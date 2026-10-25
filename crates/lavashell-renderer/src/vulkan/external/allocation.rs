// SPDX-License-Identifier: GPL-3.0-only
use super::super::skia_handoff::{SkiaImage, backend_texture};
use crate::Error;
use ash::vk;
use lavashell_vulkan::{
    Commands, Device, Image, ImageState, OpaqueFdExport, QueueFamily, Submitted,
};
use skia_safe::gpu::{self, vk as skvk};
use std::cell::{Cell, RefCell};
use std::sync::Arc;

/// The layout and queue family in which a producer releases a scene image for import, and in which the importer
/// hands the image back to the producer.
pub(super) const RELEASED: ImageState =
    ImageState::new(vk::ImageLayout::GENERAL, QueueFamily::External);

const ACQUIRED: ImageState = ImageState::new(vk::ImageLayout::GENERAL, QueueFamily::Device);

/// The side that has an imported scene image at the moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holder {
    /// The producer has the image and has released it for the next import.
    Producer,
    /// The renderer has acquired the image for a scene.
    Renderer,
    /// The queue of the renderer is handing the image back to the producer.
    HandBack,
}

/// An imported scene image, with its Skia texture and the command buffers that acquire the image and hand it back.
pub(super) struct Allocation {
    pub texture: gpu::BackendTexture,
    image: Arc<Image>,
    holder: Cell<Holder>,
    /// Command buffers whose submissions are complete, reused for the next acquire and hand-back.
    spare: RefCell<Vec<Commands>>,
}

impl Allocation {
    pub fn import(device: &Arc<Device>, export: &OpaqueFdExport) -> Result<Self, Error> {
        let description = export.description();
        if description.format != vk::Format::R8G8B8A8_SRGB {
            return Err(Error::SceneFormat(description.format));
        }
        let extent = description.extent;
        let (Ok(width), Ok(height)) = (i32::try_from(extent.width), i32::try_from(extent.height))
        else {
            return Err(Error::SceneSize(extent));
        };
        let image = Image::import(device, export).map_err(Error::ImportScene)?;
        let skia = SkiaImage {
            image: image.handle(),
            layout: skvk::ImageLayout::GENERAL,
            format: skvk::Format::R8G8B8A8_SRGB,
            usage: description.usage,
            queue_family: device.queue_family(),
            conversion: None,
        };
        // SAFETY: Skia receives the image handle without a lifetime. The allocation has a reference to the image.
        // Skia reads the image only while a lease has a reference to the allocation. An allocation is dropped only
        // after the GPU work submitted before the drop is complete and after its texture is purged from the Skia
        // cache.
        let texture = unsafe { backend_texture(&skia, (width, height), "external scene") }?;
        Ok(Self {
            texture,
            image,
            holder: Cell::new(Holder::Producer),
            spare: RefCell::new(Vec::new()),
        })
    }

    /// Acquires the image from the producer on the queue of the renderer.
    ///
    /// # Errors
    /// Fails when the renderer has the image already, or when Vulkan cannot record or submit the acquire.
    ///
    /// # Safety
    /// The producer must have released the image in `RELEASED`, and that release must be complete.
    pub unsafe fn acquire(&self) -> Result<Submitted, Error> {
        if self.holder.get() != Holder::Producer {
            return Err(Error::SceneInUse);
        }
        let mut recording = (self.commands())
            .and_then(Commands::record)
            .map_err(Error::AcquireScene)?;
        // SAFETY: lavashell-vulkan cannot see the state in which another device left the image, so `assume` declares
        // that state. The caller guarantees that the producer released the image in `RELEASED` and that the release is
        // complete.
        let submitted = unsafe { recording.assume(&self.image, RELEASED) }
            .and_then(|()| recording.transition(&self.image, ACQUIRED))
            .and_then(|()| recording.finish()?.submit())
            .map_err(Error::AcquireScene)?;
        self.holder.set(Holder::Renderer);
        Ok(submitted)
    }

    /// Hands the image back to the producer in `RELEASED`.
    ///
    /// # Errors
    /// Fails when Vulkan cannot record or submit the hand-back.
    ///
    /// # Safety
    /// The work submitted before must leave the image in `GENERAL` on the queue of the renderer.
    pub unsafe fn release(&self) -> Result<Submitted, Error> {
        let mut recording = (self.commands())
            .and_then(Commands::record)
            .map_err(Error::ReleaseScene)?;
        // SAFETY: Skia tracks the layout of the image separately from lavashell-vulkan, so `assume` declares that
        // layout. The caller guarantees that the work submitted before leaves the image in `GENERAL` on this queue.
        let submitted = unsafe { recording.assume(&self.image, ACQUIRED) }
            .and_then(|()| recording.transition(&self.image, RELEASED))
            .and_then(|()| recording.finish()?.submit())
            .map_err(Error::ReleaseScene)?;
        self.holder.set(Holder::HandBack);
        Ok(submitted)
    }

    /// Stores `commands`, whose acquire and hand-back are complete, for reuse, and marks the image as being with the
    /// producer again.
    pub fn returned(&self, commands: impl IntoIterator<Item = Commands>) {
        self.spare.borrow_mut().extend(commands);
        self.holder.set(Holder::Producer);
    }

    fn commands(&self) -> Result<Commands, lavashell_vulkan::Error> {
        let spare = self.spare.borrow_mut().pop();
        spare.map_or_else(|| Commands::new(self.image.device()), Ok)
    }
}
