// SPDX-License-Identifier: GPL-3.0-only
//! The import of AOSP hardware buffers drawn by another process, for sampling by the renderer. Before the first read,
//! the renderer acquires a buffer from the foreign queue family. After the reads, the renderer hands the buffer back
//! to that queue family.

mod memory;

use super::buffer_format::{ImportFormat, import_format};
use super::completion::drop_completed;
use super::skia_handoff::{SkiaImage, backend_texture, submit_recorded};
use super::{Completion, VulkanRenderer};
use crate::Error;
use crate::{BufferAlpha, BufferFlag, HardwareBufferDescription, StreamPixels, StreamRejection};
use ash::android::external_memory_android_hardware_buffer as hardware_buffer;
use ash::ext::queue_family_foreign;
use ash::khr::external_semaphore_fd;
use ash::vk;
use lavashell_core::prelude::PixelSize;
use lavashell_vulkan::{Device, ExtensionSet, Image};
pub use memory::FormatProperties;
use memory::{BufferMemory, BufferProperties, read_properties};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferUsage};
use ndk::hardware_buffer_format::HardwareBufferFormat;
use skia_safe::gpu::{self, SurfaceOrigin, vk as skvk};
use skia_safe::{AlphaType, ColorSpace, ColorType};
use std::cell::RefCell;
use std::ffi::CStr;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;

pub(super) const EXTENSIONS: [&CStr; 3] = [
    hardware_buffer::NAME,
    external_semaphore_fd::NAME,
    queue_family_foreign::NAME,
];

pub enum Sampling {
    Sampled(HardwareBufferProperties),
    Rejected(StreamRejection),
}

/// How the renderer samples one hardware buffer: the Vulkan properties, the size, the alpha and the import format of
/// the buffer. The Vulkan properties have a reference to the buffer, so the buffer exists as long as they do.
pub struct HardwareBufferProperties {
    vulkan: BufferMemory,
    size: PixelSize,
    alpha: BufferAlpha,
    format: ImportFormat,
}

/// A hardware buffer imported as a Vulkan image, with its Skia texture. Dropping a `HardwareBufferImport` destroys
/// the image and its memory once the GPU has finished the work submitted before the next
/// `HardwareBufferImporter::poll`.
pub struct HardwareBufferImport {
    texture: gpu::BackendTexture,
    alpha: BufferAlpha,
    image: Option<Arc<Image>>,
    retired: Rc<Retired>,
}

impl Drop for HardwareBufferImport {
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
            log::error!(
                "hardware buffer imports are dropped without finishing their GPU work: {error}"
            );
        }
    }
}

/// The images of retired imports, destroyed once the GPU has finished the work submitted before `completion`.
struct InFlight {
    completion: Completion,
    _images: Vec<Arc<Image>>,
}

/// Imports hardware buffers and hands them back to the producer with release fences, which signal once the GPU has
/// finished reading. No method of the importer waits for the GPU.
pub struct HardwareBufferImporter {
    device: Arc<Device>,
    in_flight: Vec<InFlight>,
    retired: Rc<Retired>,
}

impl VulkanRenderer {
    /// # Errors
    /// Fails when the device lacks an extension that the import of hardware buffers needs.
    pub fn hardware_buffer_importer(&self) -> Result<HardwareBufferImporter, Error> {
        let device = &self.device;
        if let Some(missing) = EXTENSIONS.into_iter().find(|name| !device.contains(name)) {
            return Err(Error::MissingExtension(missing));
        }
        Ok(HardwareBufferImporter {
            device: device.clone(),
            in_flight: Vec::new(),
            retired: Rc::new(Retired {
                device: device.clone(),
                images: RefCell::default(),
            }),
        })
    }
}

impl HardwareBufferImporter {
    /// Determines how the renderer samples `buffer`, without GPU work. It rejects a buffer with several layers, a
    /// complete mipmap chain, protected content or no GPU sampled usage, a buffer that the driver cannot describe,
    /// and a buffer in a format rejected by `import_format`.
    ///
    /// # Errors
    /// Fails when Vulkan cannot read the properties of the buffer for another reason than an unusable buffer.
    pub fn properties(&self, buffer: &HardwareBuffer) -> Result<Sampling, Error> {
        let description = description(buffer);
        if let Some(rejection) = description.rejection() {
            return Ok(Sampling::Rejected(rejection));
        }
        let unusable = Ok(Sampling::Rejected(StreamRejection::UnusableBuffer));
        let vulkan = match read_properties(&self.device, buffer)? {
            BufferProperties::Usable(vulkan) => vulkan,
            BufferProperties::InvalidHandle => return unusable,
        };
        let buffer_format = vulkan.format();
        let ycbcr_conversion = self.device.features().sampler_ycbcr_conversion;
        let format = match import_format(buffer_format, ycbcr_conversion) {
            Ok(format) => format,
            Err(rejection) => return Ok(Sampling::Rejected(rejection)),
        };
        if vulkan.memory_type_bits() == 0 {
            return unusable;
        }
        let alpha_ignored =
            buffer_format.sampler_ycbcr_conversion_components.a == vk::ComponentSwizzle::ONE;
        let alpha = if alpha_ignored {
            BufferAlpha::Undefined
        } else {
            description.alpha
        };
        Ok(Sampling::Sampled(HardwareBufferProperties {
            vulkan,
            size: description.size,
            alpha,
            format,
        }))
    }

    /// The format properties that the driver reports for `buffer`.
    ///
    /// # Errors
    /// Fails when Vulkan cannot read the properties of the buffer, or when the driver cannot import the buffer.
    pub fn format_properties(&self, buffer: &HardwareBuffer) -> Result<FormatProperties, Error> {
        match read_properties(&self.device, buffer)? {
            BufferProperties::Usable(properties) => Ok(*properties.format()),
            BufferProperties::InvalidHandle => Err(Error::InvalidHardwareBuffer),
        }
    }

    /// Imports the buffer of `properties` as a Vulkan image with a Skia texture.
    ///
    /// # Errors
    /// Fails when the buffer is too large for Skia, or when Vulkan cannot import the buffer.
    pub fn import(
        &self,
        properties: &HardwareBufferProperties,
    ) -> Result<HardwareBufferImport, Error> {
        let size = properties.size;
        let (width, height) = size.signed().ok_or(Error::HardwareBufferSize(size))?;
        let format = properties.format;
        let extent = vk::Extent2D {
            width: size.width,
            height: size.height,
        };
        let image = Image::import_memory(&self.device, extent, format.usage(), properties)
            .map_err(Error::ImportHardwareBuffer)?;
        // The producer releases the buffer to the foreign queue family in the undefined layout, as the hardware buffer
        // import inside Skia expects. The contents of the buffer are preserved despite the undefined layout.
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
        let texture = unsafe { backend_texture(&skia, (width, height), "hardware buffer") }?;
        Ok(HardwareBufferImport {
            texture,
            alpha: properties.alpha,
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
        import: &Rc<HardwareBufferImport>,
    ) -> Result<StreamPixels, Error> {
        let (color, alpha) = match import.alpha {
            BufferAlpha::Premultiplied => (ColorType::RGBA8888, AlphaType::Premul),
            BufferAlpha::Undefined => (ColorType::RGB888x, AlphaType::Opaque),
        };
        let image = gpu::images::borrow_texture_from(
            &mut gpu.context,
            &import.texture,
            SurfaceOrigin::TopLeft,
            color,
            alpha,
            ColorSpace::new_srgb(),
        )
        .ok_or(Error::WrapHardwareBuffer)?;
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
        imports: impl IntoIterator<Item = &'a HardwareBufferImport>,
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
            return Err(Error::HandBackHardwareBuffers);
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

/// The size, the layer count, the usage flags and the alpha of `buffer`, read from the description that AOSP returns
/// for the buffer.
fn description(buffer: &HardwareBuffer) -> HardwareBufferDescription {
    let description = buffer.describe();
    let flags = [
        (
            HardwareBufferUsage::GPU_SAMPLED_IMAGE,
            BufferFlag::GpuSampled,
        ),
        (
            HardwareBufferUsage::PROTECTED_CONTENT,
            BufferFlag::Protected,
        ),
        (
            HardwareBufferUsage::GPU_MIPMAP_COMPLETE,
            BufferFlag::MipmapComplete,
        ),
    ];
    HardwareBufferDescription {
        size: PixelSize::new(description.width, description.height),
        layers: description.layers,
        usage: (flags.into_iter())
            .filter(|&(usage, _)| description.usage.contains(usage))
            .map(|(_, flag)| flag)
            .collect(),
        alpha: if description.format == HardwareBufferFormat::R8G8B8X8_UNORM {
            BufferAlpha::Undefined
        } else {
            BufferAlpha::Premultiplied
        },
    }
}
