// SPDX-License-Identifier: GPL-3.0-only

use super::buffer_format::{ForeignFormat, ImportFormat, import_format};
use super::completion::finish_completed;
use super::deferred_release::{DeferredRelease, ReleaseQueue};
use super::skia_handoff::{SkiaImage, backend_texture, set_texture_states};
use super::{Completion, VulkanRenderer};
use crate::Error;
use crate::{StreamPixels, StreamRejection};
use ash::ext::queue_family_foreign;
use ash::khr::external_semaphore_fd;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_vulkan::{
    Allocation, Device, ExtensionSet, Image, ImageFormat, ImageMemory, Sharing,
};
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{AlphaType, ColorSpace, ColorType};
use std::ffi::CStr;
use std::num::{NonZeroU32, NonZeroU64};
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;

pub const FOREIGN_IMPORT_EXTENSIONS: [&CStr; 2] =
    [external_semaphore_fd::NAME, queue_family_foreign::NAME];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForeignAlpha {
    Premultiplied,
    Undefined,
}

#[derive(Clone, Copy, Debug)]
pub struct ForeignMemoryDescription {
    pub handle_type: vk::ExternalMemoryHandleTypeFlags,
    pub allocation_size: vk::DeviceSize,
    pub memory_type_bits: u32,
    pub size: PixelSize,
    pub alpha: ForeignAlpha,
    pub format: ForeignFormat,
}

/// # Safety
/// The structure chained by `memory_chain` imports one live allocation of the handle type in `description`. The
/// other fields of `description` describe that allocation as the driver reports it. Every chained structure lives
/// through the call that receives the chain.
pub unsafe trait ForeignMemory {
    const EXTENSIONS: &'static [&'static CStr];

    fn description(&self) -> &ForeignMemoryDescription;

    fn memory_chain(
        &self,
        info: vk::MemoryAllocateInfo<'_>,
        allocate: impl FnOnce(
            &vk::MemoryAllocateInfo<'_>,
        ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>,
    ) -> Result<vk::DeviceMemory, lavashell_vulkan::Error>;
}

pub enum Sampling<S> {
    Sampled(S),
    Rejected(StreamRejection),
}

pub struct SampledMemory<M> {
    memory: M,
    alpha: ForeignAlpha,
    format: ImportFormat,
    memory_types: NonZeroU32,
}

struct VulkanMemory<'a, M>(&'a SampledMemory<M>);

// SAFETY: The Vulkan crate cannot check memory of a platform that it does not know, so its import trait is unsafe.
// The memory chain, the handle type and the allocation size come from `M`, whose `ForeignMemory` contract guarantees
// them. `allocation` returns the lowest memory type in the bits described by `M`, which `sampling` found not to be
// empty. `image_chain` chains the external format described by `M` exactly where `format` returns
// `ImageFormat::External`.
unsafe impl<M: ForeignMemory> ImageMemory for VulkanMemory<'_, M> {
    fn sharing(&self) -> Sharing {
        Sharing::Import(self.0.memory.description().handle_type)
    }

    fn required_extensions(&self) -> &'static [&'static CStr] {
        M::EXTENSIONS
    }

    fn allocation(
        &self,
        _device: &Device,
        _requirements: &vk::MemoryRequirements,
    ) -> Result<Allocation, lavashell_vulkan::Error> {
        let sampled = self.0;
        Ok(Allocation {
            size: sampled.memory.description().allocation_size,
            memory_type: sampled.memory_types.trailing_zeros(),
        })
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
            ImageFormat::External => {
                let format = self.0.memory.description().format.external_format;
                let mut external = vk::ExternalFormatANDROID::default()
                    .external_format(format.map_or(0, NonZeroU64::get));
                create(&info.push_next(&mut external))
            }
        }
    }
}

impl<M: ForeignMemory> VulkanMemory<'_, M> {
    fn format(&self) -> ImageFormat {
        self.0.memory.description().format.image_format()
    }
}

pub struct ForeignImport {
    texture: gpu::BackendTexture,
    alpha: ForeignAlpha,
    image: Option<Arc<Image>>,
    retired: DeferredRelease<Arc<Image>>,
}

impl Drop for ForeignImport {
    fn drop(&mut self) {
        if let Some(image) = self.image.take()
            && let Err(unpolled) = self.retired.defer(image)
        {
            drop_after_idle(vec![unpolled]);
        }
    }
}

fn drop_after_idle(images: Vec<Arc<Image>>) {
    if let Some(image) = images.first()
        && let Err(error) = image.device().wait_idle()
    {
        log::error!("foreign imports are dropped without finishing their GPU work: {error}");
    }
    drop(images);
}

struct InFlight {
    completion: Completion,
    images: Vec<Arc<Image>>,
}

impl InFlight {
    fn finish(self) {
        drop(self.images);
    }
}

pub struct ForeignImporter {
    device: Arc<Device>,
    in_flight: Vec<InFlight>,
    retired: ReleaseQueue<Arc<Image>>,
}

impl Drop for ForeignImporter {
    fn drop(&mut self) {
        drop_after_idle(self.retired.take_deferred());
    }
}

impl VulkanRenderer {
    fn wrap_foreign_texture(&mut self, import: &ForeignImport) -> Option<skia_safe::Image> {
        let (color, alpha) = match import.alpha {
            ForeignAlpha::Premultiplied => (ColorType::RGBA8888, AlphaType::Premul),
            ForeignAlpha::Undefined => (ColorType::RGB888x, AlphaType::Opaque),
        };
        gpu::images::borrow_texture_from(
            &mut self.context,
            &import.texture,
            SurfaceOrigin::TopLeft,
            color,
            alpha,
            ColorSpace::new_srgb(),
        )
    }

    fn release_foreign_textures<'a>(
        &mut self,
        imports: impl IntoIterator<Item = &'a ForeignImport>,
    ) -> Result<(), Error> {
        let foreign = skvk::mutable_texture_states::new_vulkan(
            skvk::ImageLayout::UNDEFINED,
            vk::QUEUE_FAMILY_FOREIGN_EXT,
        );
        let textures = imports.into_iter().map(|import| &import.texture);
        set_texture_states(
            &mut self.context,
            textures,
            &foreign,
            Error::HandBackForeign,
        )
    }

    pub fn foreign_importer<M: ForeignMemory>(&self) -> Result<ForeignImporter, Error> {
        let device = self.context.device();
        (device.require(&FOREIGN_IMPORT_EXTENSIONS))
            .and_then(|()| device.require(M::EXTENSIONS))
            .map_err(Error::ForeignExtensions)?;
        Ok(ForeignImporter {
            device: device.clone(),
            in_flight: Vec::new(),
            retired: ReleaseQueue::default(),
        })
    }
}

impl ForeignImporter {
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    #[must_use]
    pub fn sampling<M: ForeignMemory>(&self, memory: M) -> Sampling<SampledMemory<M>> {
        let description = memory.description();
        let foreign = &description.format;
        let ycbcr_conversion = self.device.features().sampler_ycbcr_conversion;
        let format = match import_format(foreign, ycbcr_conversion) {
            Ok(format) => format,
            Err(rejection) => return Sampling::Rejected(rejection),
        };
        let Some(memory_types) = NonZeroU32::new(description.memory_type_bits) else {
            return Sampling::Rejected(StreamRejection::UnusableBuffer);
        };
        let alpha = if foreign.components.a == vk::ComponentSwizzle::ONE {
            ForeignAlpha::Undefined
        } else {
            description.alpha
        };
        Sampling::Sampled(SampledMemory {
            memory,
            alpha,
            format,
            memory_types,
        })
    }

    pub fn import<M: ForeignMemory>(
        &self,
        sampled: &SampledMemory<M>,
    ) -> Result<ForeignImport, Error> {
        let size = sampled.memory.description().size;
        let (width, height) = size.signed().ok_or(Error::ForeignSize(size))?;
        let format = sampled.format;
        let extent = vk::Extent2D {
            width: size.width,
            height: size.height,
        };
        let memory = VulkanMemory(sampled);
        let image = Image::import_memory(
            &self.device,
            memory.format(),
            extent,
            format.usage(),
            &memory,
        )
        .map_err(Error::ImportForeign)?;
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
            retired: self.retired.create_handle(),
        })
    }

    pub fn acquire(
        &self,
        gpu: &mut VulkanRenderer,
        import: &Rc<ForeignImport>,
    ) -> Result<StreamPixels, Error> {
        let image = gpu.wrap_foreign_texture(import).ok_or(Error::WrapForeign)?;
        Ok(StreamPixels {
            image,
            import: Some(import.clone()),
        })
    }

    pub fn release<'a>(
        &mut self,
        gpu: &mut VulkanRenderer,
        imports: impl IntoIterator<Item = &'a ForeignImport>,
    ) -> Result<Option<OwnedFd>, Error> {
        gpu.release_foreign_textures(imports)?;
        let signal = (self.device.signal_sync_fd()).map_err(Error::SignalRelease)?;
        self.in_flight.push(InFlight {
            completion: Completion::of_submission(signal.submission),
            images: self.retired.take_deferred(),
        });
        Ok(signal.fence)
    }

    pub fn completion(&self) -> Result<Completion, Error> {
        Completion::after_submitted(&self.device)
    }

    pub fn poll(&mut self) -> Result<(), Error> {
        finish_completed(
            &mut self.in_flight,
            |in_flight| &in_flight.completion,
            InFlight::finish,
        )?;
        let retired = self.retired.take_deferred();
        if !retired.is_empty() {
            let completion = Completion::after_submitted(&self.device)?;
            self.in_flight.push(InFlight {
                completion,
                images: retired,
            });
        }
        Ok(())
    }
}
