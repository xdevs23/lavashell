// SPDX-License-Identifier: GPL-3.0-only
//! A simulation of the foreign import used by a stream and of the hardware-buffer memory that it imports. It imports
//! the buffers of a simulated reader as Skia images filled with the color of each buffer.

use super::image::{HardwareBuffer, HardwareBufferFormat};
use super::reader::{Event, Log, signalled_fence};
use super::{Gpu, VulkanRenderer};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::Error as RendererError;
use lavashell_renderer::vulkan::Status;
use lavashell_renderer::{
    ForeignAlpha, Sampling, StreamPixels, StreamRejection, UnsampledFormat, skia,
};
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub struct SampledMemory<M>(M);

pub type PropertiesError = RendererError;

/// The parts of a simulated buffer that are read by the simulated import.
pub struct HardwareBufferMemory {
    size: PixelSize,
    id: u64,
    color: skia::Color,
    format: HardwareBufferFormat,
    fails_import: bool,
    log: Log,
}

impl HardwareBufferMemory {
    pub fn importer(gpu: &VulkanRenderer) -> Result<ForeignImporter, RendererError> {
        gpu.foreign_importer()
    }

    pub fn read(
        importer: &ForeignImporter,
        buffer: &HardwareBuffer,
        size: PixelSize,
        _: ForeignAlpha,
    ) -> Result<Option<Self>, PropertiesError> {
        importer.gpu.reachable()?;
        let (buffer, log) = buffer.contents();
        Ok((!buffer.undescribable).then(|| Self {
            size,
            id: buffer.id,
            color: buffer.color,
            format: buffer.format,
            fails_import: buffer.fails_import,
            log: log.clone(),
        }))
    }
}

pub struct ForeignImport {
    id: u64,
    pixels: skia::Image,
    log: Log,
}

impl Drop for ForeignImport {
    fn drop(&mut self) {
        self.log.borrow_mut().push(Event::ImportDropped(self.id));
    }
}

/// A failure of the simulated release of imports back to their producers.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("the simulated GPU did not hand the imports back")]
    HandBack(#[source] RendererError),
    #[error("the host cannot make the release fence of the simulated imports")]
    Fence(#[source] std::io::Error),
}

pub struct Completion(Rc<Gpu>);

impl Completion {
    pub fn status(&self) -> Result<Status, RendererError> {
        self.0.reachable()?;
        Ok(if self.0.busy.get() {
            Status::Pending
        } else {
            Status::Complete
        })
    }
}

pub struct ForeignImporter {
    gpu: Rc<Gpu>,
}

impl ForeignImporter {
    pub(super) fn new(gpu: &Rc<Gpu>) -> Self {
        Self { gpu: gpu.clone() }
    }

    /// Rejects the format sampled only through a YCbCr conversion when the simulated device does not support one.
    pub fn sampling(
        &self,
        memory: HardwareBufferMemory,
    ) -> Sampling<SampledMemory<HardwareBufferMemory>> {
        if memory.format == HardwareBufferFormat::R16G16B16A16_FLOAT
            && !self.gpu.sampler_ycbcr_conversion.get()
        {
            let rejection = StreamRejection::BufferFormat(UnsampledFormat::NeedsYcbcrConversion);
            return Sampling::Rejected(rejection);
        }
        Sampling::Sampled(SampledMemory(memory))
    }

    pub fn import(
        &self,
        SampledMemory(memory): &SampledMemory<HardwareBufferMemory>,
    ) -> Result<ForeignImport, RendererError> {
        self.gpu.reachable()?;
        if memory.fails_import {
            return Err(RendererError::WrapForeign);
        }
        let size = (memory.size.signed()).ok_or(RendererError::ForeignSize(memory.size))?;
        let mut pixels = skia::Bitmap::new();
        pixels.alloc_n32_pixels(size, None);
        pixels.erase_color(memory.color);
        let log = memory.log.clone();
        log.borrow_mut().push(Event::Imported(memory.id));
        Ok(ForeignImport {
            id: memory.id,
            pixels: pixels.as_image(),
            log,
        })
    }

    pub fn acquire(
        &self,
        _: &mut VulkanRenderer,
        import: &Rc<ForeignImport>,
    ) -> Result<StreamPixels, RendererError> {
        self.gpu.reachable()?;
        import.log.borrow_mut().push(Event::Acquired(import.id));
        Ok(StreamPixels::from(import.pixels.clone()))
    }

    pub fn release<'a>(
        &mut self,
        _: &mut VulkanRenderer,
        imports: impl IntoIterator<Item = &'a ForeignImport>,
    ) -> Result<Option<OwnedFd>, ReleaseError> {
        self.gpu.reachable().map_err(ReleaseError::HandBack)?;
        for import in imports {
            import.log.borrow_mut().push(Event::HandedBack(import.id));
        }
        if self.gpu.fails_release.replace(false) {
            return Err(ReleaseError::HandBack(RendererError::HandBackForeign));
        }
        let fence = signalled_fence().map_err(ReleaseError::Fence)?;
        Ok(Some(fence))
    }

    pub fn completion(&self) -> Result<Completion, RendererError> {
        self.gpu.reachable()?;
        if self.gpu.fails_completion.get() {
            return Err(RendererError::SkiaAbandoned);
        }
        Ok(Completion(self.gpu.clone()))
    }

    /// Destroys nothing, because the simulated imports own no Vulkan objects.
    pub fn poll(&mut self) -> Result<(), RendererError> {
        self.gpu.reachable()
    }
}
