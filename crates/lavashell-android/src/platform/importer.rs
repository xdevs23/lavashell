// SPDX-License-Identifier: GPL-3.0-only
//! A simulation of the hardware buffer import used by a stream. It imports the buffers of a simulated reader as Skia
//! images filled with the color of each buffer.

use super::image::{HardwareBuffer, HardwareBufferFormat, HardwareBufferUsage};
use super::reader::{Buffer, Event, Log, signalled_fence};
use super::{Gpu, VulkanRenderer};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::Error as RendererError;
use lavashell_renderer::vulkan::Status;
use lavashell_renderer::{
    BufferAlpha, BufferFlag, HardwareBufferDescription, StreamPixels, StreamRejection,
    UnsampledFormat, skia,
};
use std::os::fd::OwnedFd;
use std::rc::Rc;

pub enum Sampling {
    Sampled(HardwareBufferProperties),
    Rejected(StreamRejection),
}

/// The parts of a simulated buffer that are read by the simulated import.
pub struct HardwareBufferProperties {
    size: PixelSize,
    id: u64,
    color: skia::Color,
    fails_import: bool,
    log: Log,
}

pub struct HardwareBufferImport {
    id: u64,
    pixels: skia::Image,
    log: Log,
}

impl Drop for HardwareBufferImport {
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

pub struct HardwareBufferImporter {
    gpu: Rc<Gpu>,
}

impl HardwareBufferImporter {
    pub(super) fn new(gpu: &Rc<Gpu>) -> Self {
        Self { gpu: gpu.clone() }
    }

    pub fn properties(&self, buffer: &HardwareBuffer) -> Result<Sampling, RendererError> {
        self.gpu.reachable()?;
        let (buffer, log) = buffer.contents();
        let description = description(buffer);
        if let Some(rejection) = description.rejection() {
            return Ok(Sampling::Rejected(rejection));
        }
        if buffer.undescribable {
            return Ok(Sampling::Rejected(StreamRejection::UnusableBuffer));
        }
        if buffer.format == HardwareBufferFormat::R16G16B16A16_FLOAT {
            let rejection = StreamRejection::BufferFormat(UnsampledFormat::NeedsYcbcrConversion);
            return Ok(Sampling::Rejected(rejection));
        }
        Ok(Sampling::Sampled(HardwareBufferProperties {
            size: description.size,
            id: buffer.id,
            color: buffer.color,
            fails_import: buffer.fails_import,
            log: log.clone(),
        }))
    }

    pub fn import(
        &self,
        properties: &HardwareBufferProperties,
    ) -> Result<HardwareBufferImport, RendererError> {
        self.gpu.reachable()?;
        if properties.fails_import {
            return Err(RendererError::WrapHardwareBuffer);
        }
        let size = properties
            .size
            .signed()
            .ok_or(RendererError::HardwareBufferSize(properties.size))?;
        let mut pixels = skia::Bitmap::new();
        pixels.alloc_n32_pixels(size, None);
        pixels.erase_color(properties.color);
        let log = properties.log.clone();
        log.borrow_mut().push(Event::Imported(properties.id));
        Ok(HardwareBufferImport {
            id: properties.id,
            pixels: pixels.as_image(),
            log,
        })
    }

    pub fn acquire(
        &self,
        _: &mut VulkanRenderer,
        import: &Rc<HardwareBufferImport>,
    ) -> Result<StreamPixels, RendererError> {
        self.gpu.reachable()?;
        import.log.borrow_mut().push(Event::Acquired(import.id));
        Ok(StreamPixels::from(import.pixels.clone()))
    }

    pub fn release<'a>(
        &mut self,
        _: &mut VulkanRenderer,
        imports: impl IntoIterator<Item = &'a HardwareBufferImport>,
    ) -> Result<Option<OwnedFd>, ReleaseError> {
        self.gpu.reachable().map_err(ReleaseError::HandBack)?;
        for import in imports {
            import.log.borrow_mut().push(Event::HandedBack(import.id));
        }
        if self.gpu.fails_release.replace(false) {
            return Err(ReleaseError::HandBack(
                RendererError::HandBackHardwareBuffers,
            ));
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

/// The description of `buffer` as the renderer reads it from a real buffer: size, layers, usage flags and alpha.
fn description(buffer: &Buffer) -> HardwareBufferDescription {
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
        size: buffer.size,
        layers: buffer.layers,
        usage: (flags.into_iter())
            .filter(|&(usage, _)| buffer.usage.contains(usage))
            .map(|(_, flag)| flag)
            .collect(),
        alpha: if buffer.format == HardwareBufferFormat::R8G8B8X8_UNORM {
            BufferAlpha::Undefined
        } else {
            BufferAlpha::Premultiplied
        },
    }
}
