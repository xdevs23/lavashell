// SPDX-License-Identifier: GPL-3.0-only
//! The import of GPU memory drawn by another process, for sampling by the renderer. Before the first read, the
//! renderer acquires the memory from the foreign queue family. After the reads, the renderer hands the memory back to
//! that queue family.

use super::buffer_format::{ForeignFormat, ImportFormat, import_format};
use super::completion::drop_completed;
use super::skia_handoff::{SkiaImage, backend_texture, submit_recorded};
use super::{Completion, VulkanRenderer};
use crate::Error;
use crate::{StreamPixels, StreamRejection};
use ash::ext::queue_family_foreign;
use ash::khr::external_semaphore_fd;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_vulkan::{Device, ExtensionSet, Image, ImageFormat, ImportedMemory};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{AlphaType, ColorSpace, ColorType};
use std::cell::RefCell;
use std::ffi::CStr;
use std::num::NonZeroU32;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;

/// The device extensions that every foreign import needs, in addition to the extensions required by the kind of memory.
pub const FOREIGN_IMPORT_EXTENSIONS: [&CStr; 2] =
    [external_semaphore_fd::NAME, queue_family_foreign::NAME];

/// What the alpha channel of foreign memory means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForeignAlpha {
    Premultiplied,
    /// The format has an alpha channel without meaning, such as an RGBX format.
    Undefined,
}

/// Memory of another process, imported and sampled by the renderer.
///
/// # Safety
/// The structure chained by `memory_chain` imports one live allocation of the handle type returned by `handle_type`.
/// `allocation_size`, `memory_type_bits`, `size` and `foreign_format` describe that allocation as the driver reports
/// it. `external_format_chain` chains the external format of `foreign_format`. Every chained structure lives through
/// the call that receives the chain.
pub unsafe trait ForeignMemory {
    /// The device extensions that the import of this kind of memory needs, in addition to `FOREIGN_IMPORT_EXTENSIONS`.
    const EXTENSIONS: &'static [&'static CStr];

    fn handle_type(&self) -> vk::ExternalMemoryHandleTypeFlags;

    fn allocation_size(&self) -> vk::DeviceSize;

    /// The memory types that can contain the allocation, one bit per memory type index.
    fn memory_type_bits(&self) -> u32;

    /// Calls `allocate` with `info`, after chaining to `info` the structure that imports the memory.
    ///
    /// # Errors
    /// Fails when `allocate` fails.
    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(
            &vk::MemoryAllocateInfo<'_>,
        ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>,
    ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>;

    /// Calls `create` with `info` for an image in the external format, after chaining that format to `info`.
    ///
    /// # Errors
    /// Fails when `create` fails.
    fn external_format_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, lavashell_vulkan::Error>,
    ) -> Result<vk::Image, lavashell_vulkan::Error>;

    fn size(&self) -> PixelSize;

    /// The alpha defined by the platform format of the memory. The renderer samples the alpha as `Undefined` wherever
    /// the driver maps alpha to ONE.
    fn alpha(&self) -> ForeignAlpha;

    fn foreign_format(&self) -> &ForeignFormat;
}

pub enum Sampling<S> {
    Sampled(S),
    Rejected(StreamRejection),
}

/// How the renderer samples foreign memory: the memory, its alpha, its import format and the memory types that can
/// contain it.
pub struct SampledMemory<M> {
    memory: M,
    alpha: ForeignAlpha,
    format: ImportFormat,
    memory_types: NonZeroU32,
}

/// Sampled foreign memory in the form imported by the Vulkan crate.
struct VulkanMemory<'a, M>(&'a SampledMemory<M>);

// SAFETY: The Vulkan crate cannot check memory of a platform that it does not know, so its import trait is unsafe.
// The chains, the handle type and the allocation size come from `M`, whose `ForeignMemory` contract guarantees them.
// `allocation` returns the lowest memory type in the bits reported by `M`, which `sampling` found not to be empty.
// `image_chain` lets `M` chain its external format exactly where `format` returns `ImageFormat::External`.
unsafe impl<M: ForeignMemory> ImportedMemory for VulkanMemory<'_, M> {
    fn required_extensions(&self) -> &'static [&'static CStr] {
        M::EXTENSIONS
    }

    fn handle_type(&self) -> vk::ExternalMemoryHandleTypeFlags {
        self.0.memory.handle_type()
    }

    fn format(&self) -> ImageFormat {
        self.0.memory.foreign_format().image_format()
    }

    fn allocation(
        &self,
        _requirements: &vk::MemoryRequirements,
    ) -> Result<(vk::DeviceSize, u32), lavashell_vulkan::Error> {
        let sampled = self.0;
        let memory_type = sampled.memory_types.trailing_zeros();
        Ok((sampled.memory.allocation_size(), memory_type))
    }

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(
            &vk::MemoryAllocateInfo<'_>,
        ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>,
    ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error> {
        self.0.memory.memory_chain(info, allocate)
    }

    fn image_chain(
        &self,
        info: vk::ImageCreateInfo<'_>,
        create: impl FnOnce(&vk::ImageCreateInfo<'_>) -> Result<vk::Image, lavashell_vulkan::Error>,
    ) -> Result<vk::Image, lavashell_vulkan::Error> {
        match self.format() {
            ImageFormat::Known(_) => create(&info),
            ImageFormat::External => self.0.memory.external_format_chain(info, create),
        }
    }
}

/// Foreign memory imported as a Vulkan image, with its Skia texture. Dropping a `ForeignImport` destroys the image and
/// its memory once the GPU has finished the work submitted before the next `ForeignImporter::poll`.
pub struct ForeignImport {
    texture: gpu::BackendTexture,
    alpha: ForeignAlpha,
    image: Option<Arc<Image>>,
    retired: Rc<Retired>,
}

impl Drop for ForeignImport {
    fn drop(&mut self) {
        self.retired.images.borrow_mut().extend(self.image.take());
    }
}

/// The images of the imports dropped since the last release or poll. After the importer and every import are
/// dropped, nothing polls these images anymore. Dropping `Retired` then waits until the device is idle.
struct Retired {
    device: Arc<Device>,
    images: RefCell<Vec<Arc<Image>>>,
}

impl Drop for Retired {
    fn drop(&mut self) {
        if self.images.get_mut().is_empty() {
            return;
        }
        if let Err(error) = self.device.wait_idle() {
            log::error!("foreign imports are dropped without finishing their GPU work: {error}");
        }
    }
}

/// The images of retired imports, destroyed once the GPU has finished the work submitted before `completion`.
struct InFlight {
    completion: Completion,
    _images: Vec<Arc<Image>>,
}

/// Imports foreign memory and hands it back to the producer with release fences, which signal once the GPU has
/// finished reading. No method of the importer waits for the GPU.
pub struct ForeignImporter {
    device: Arc<Device>,
    in_flight: Vec<InFlight>,
    retired: Rc<Retired>,
}

impl VulkanRenderer {
    /// An importer of foreign memory of the kind `M`.
    ///
    /// # Errors
    /// Fails when the device lacks an extension in `FOREIGN_IMPORT_EXTENSIONS` or in `M::EXTENSIONS`.
    pub fn foreign_importer<M: ForeignMemory>(&self) -> Result<ForeignImporter, Error> {
        let device = &self.device;
        (device.require(&FOREIGN_IMPORT_EXTENSIONS))
            .and_then(|()| device.require(M::EXTENSIONS))
            .map_err(Error::ForeignExtensions)?;
        Ok(ForeignImporter {
            device: device.clone(),
            in_flight: Vec::new(),
            retired: Rc::new(Retired {
                device: device.clone(),
                images: RefCell::default(),
            }),
        })
    }
}

impl ForeignImporter {
    /// The device used for the import. A platform reads the properties of foreign memory from this device.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Determines how the renderer samples `memory`, without GPU work. It rejects memory in a format rejected by
    /// `import_format`, and memory that no memory type can contain.
    #[must_use]
    pub fn sampling<M: ForeignMemory>(&self, memory: M) -> Sampling<SampledMemory<M>> {
        let foreign = memory.foreign_format();
        let ycbcr_conversion = self.device.features().sampler_ycbcr_conversion;
        let format = match import_format(foreign, ycbcr_conversion) {
            Ok(format) => format,
            Err(rejection) => return Sampling::Rejected(rejection),
        };
        let Some(memory_types) = NonZeroU32::new(memory.memory_type_bits()) else {
            return Sampling::Rejected(StreamRejection::UnusableBuffer);
        };
        let alpha = if foreign.components.a == vk::ComponentSwizzle::ONE {
            ForeignAlpha::Undefined
        } else {
            memory.alpha()
        };
        Sampling::Sampled(SampledMemory {
            memory,
            alpha,
            format,
            memory_types,
        })
    }

    /// Imports the memory of `sampled` as a Vulkan image with a Skia texture.
    ///
    /// # Errors
    /// Fails when the memory is too large for Skia, or when Vulkan cannot import the memory.
    pub fn import<M: ForeignMemory>(
        &self,
        sampled: &SampledMemory<M>,
    ) -> Result<ForeignImport, Error> {
        let size = sampled.memory.size();
        let (width, height) = size.signed().ok_or(Error::ForeignSize(size))?;
        let format = sampled.format;
        let extent = vk::Extent2D {
            width: size.width,
            height: size.height,
        };
        let memory = VulkanMemory(sampled);
        let image = Image::import_memory(&self.device, extent, format.usage(), &memory)
            .map_err(Error::ImportForeign)?;
        // The producer releases the memory to the foreign queue family in the undefined layout, as Skia's own import of
        // foreign memory expects. The contents of the memory are preserved despite the undefined layout.
        let skia = SkiaImage {
            image: image.handle(),
            layout: skvk::ImageLayout::UNDEFINED,
            format: format.skia_format(),
            usage: format.usage(),
            queue_family: vk::QUEUE_FAMILY_FOREIGN_EXT,
            conversion: format.conversion(),
        };
        // SAFETY: Skia receives the image without a lifetime. The import has a reference to the image, and every Skia
        // image made from the import by `acquire` has a reference to the import. Once the import is dropped, the
        // image moves to `retired`. The image is destroyed after the GPU has finished the work submitted before the
        // next poll, or after the device is idle when nothing polls anymore.
        let texture = unsafe { backend_texture(&skia, (width, height), "foreign memory") }?;
        Ok(ForeignImport {
            texture,
            alpha: sampled.alpha,
            image: Some(image),
            retired: self.retired.clone(),
        })
    }

    /// The pixels of `import` for a frame that the producer has finished writing. The pixels have a reference to the
    /// import, so the import is not dropped while they exist. Skia acquires the image from the foreign queue family
    /// in a barrier before the first read. `release` hands the image back.
    ///
    /// # Errors
    /// Fails when Skia cannot wrap the import.
    pub fn acquire(
        &self,
        gpu: &mut VulkanRenderer,
        import: &Rc<ForeignImport>,
    ) -> Result<StreamPixels, Error> {
        let (color, alpha) = match import.alpha {
            ForeignAlpha::Premultiplied => (ColorType::RGBA8888, AlphaType::Premul),
            ForeignAlpha::Undefined => (ColorType::RGB888x, AlphaType::Opaque),
        };
        let image = gpu::images::borrow_texture_from(
            &mut gpu.context,
            &import.texture,
            SurfaceOrigin::TopLeft,
            color,
            alpha,
            ColorSpace::new_srgb(),
        )
        .ok_or(Error::WrapForeign)?;
        Ok(StreamPixels {
            image,
            _owner: Some(import.clone()),
        })
    }

    /// Hands `imports` back to the foreign queue family, and returns a sync fence that signals
    /// once the GPU has finished the work submitted so far, every read of the imports included.
    /// `None` when that work is already finished.
    ///
    /// # Errors
    /// Fails when Skia cannot hand an import back, or when Vulkan cannot signal the fence. Skia fails to hand an
    /// import back after the loss of the device.
    pub fn release<'a>(
        &mut self,
        gpu: &mut VulkanRenderer,
        imports: impl IntoIterator<Item = &'a ForeignImport>,
    ) -> Result<Option<OwnedFd>, Error> {
        let foreign = skvk::mutable_texture_states::new_vulkan(
            skvk::ImageLayout::UNDEFINED,
            vk::QUEUE_FAMILY_FOREIGN_EXT,
        );
        let context = &mut *gpu.context;
        let mut refused = false;
        for import in imports {
            refused |= !context.set_backend_texture_state(&import.texture, &foreign);
        }
        // The barriers that hand the imports back, and every read before them, are submitted before the signal.
        submit_recorded(context)?;
        if refused {
            return Err(Error::HandBackForeign);
        }
        let signal = (self.device.signal_sync_fd()).map_err(Error::SignalRelease)?;
        self.in_flight.push(InFlight {
            completion: Completion::of_submission(signal.submission),
            _images: self.retired.images.take(),
        });
        Ok(signal.fence)
    }

    /// A completion that signals once all GPU work submitted to the queue so far is complete.
    ///
    /// # Errors
    /// Fails when Vulkan cannot submit the completion.
    pub fn completion(&self) -> Result<Completion, Error> {
        Completion::after_submitted(&self.device)
    }

    /// Destroys the images of dropped imports that the GPU no longer uses, without waiting.
    ///
    /// # Errors
    /// Fails when Vulkan cannot read the status of GPU work or cannot submit a completion. Both happen after the loss
    /// of the device. The retired images are then handled by the next poll.
    pub fn poll(&mut self) -> Result<(), Error> {
        drop_completed(&mut self.in_flight, |in_flight| &in_flight.completion)?;
        if !self.retired.images.borrow().is_empty() {
            let completion = Completion::after_submitted(&self.device)?;
            self.in_flight.push(InFlight {
                completion,
                _images: self.retired.images.take(),
            });
        }
        Ok(())
    }
}
