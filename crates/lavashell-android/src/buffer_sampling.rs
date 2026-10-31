// SPDX-License-Identifier: GPL-3.0-only
//! How the renderer samples a hardware buffer, decided from the description of the buffer and the properties that
//! Vulkan returns for it.

use crate::platform::ndk::hardware_buffer::{HardwareBuffer, HardwareBufferUsage};
use crate::platform::ndk::hardware_buffer_format::HardwareBufferFormat;
use crate::platform::{
    ForeignImporter, HardwareBufferMemory, PropertiesError, SampledMemory, Sampling,
};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{ForeignAlpha, StreamRejection};
use std::collections::BTreeSet;

/// The size, the layer count, the usage flags and the alpha of a hardware buffer, read from the description that AOSP
/// returns for the buffer.
struct HardwareBufferDescription {
    size: PixelSize,
    layers: u32,
    usage: BTreeSet<BufferFlag>,
    alpha: ForeignAlpha,
}

/// A usage flag of a hardware buffer that affects whether the buffer can be sampled by the import.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BufferFlag {
    GpuSampled,
    Protected,
    MipmapComplete,
}

impl HardwareBufferDescription {
    fn of(buffer: &HardwareBuffer) -> Self {
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
        Self {
            size: PixelSize::new(description.width, description.height),
            layers: description.layers,
            usage: (flags.into_iter())
                .filter(|&(usage, _)| description.usage.contains(usage))
                .map(|(_, flag)| flag)
                .collect(),
            alpha: if description.format == HardwareBufferFormat::R8G8B8X8_UNORM {
                ForeignAlpha::Undefined
            } else {
                ForeignAlpha::Premultiplied
            },
        }
    }

    /// The rejection of a buffer that cannot be sampled by the import: a buffer with several
    /// layers, a complete mipmap chain, protected content or without GPU sampled usage.
    fn rejection(&self) -> Option<StreamRejection> {
        let usage = &self.usage;
        let sampled = self.layers == 1 && usage.contains(&BufferFlag::GpuSampled);
        (!sampled
            || usage.contains(&BufferFlag::Protected)
            || usage.contains(&BufferFlag::MipmapComplete))
        .then_some(StreamRejection::UnusableBuffer)
    }
}

/// Determines how the renderer samples `buffer`, without GPU work. It rejects a buffer whose description is unusable,
/// a buffer that the driver cannot import, and a buffer rejected by `ForeignImporter::sampling`.
///
/// # Errors
/// Fails when Vulkan cannot read the properties of the buffer for another reason than an unusable buffer.
pub(crate) fn sampling(
    importer: &ForeignImporter,
    buffer: &HardwareBuffer,
) -> Result<Sampling<SampledMemory<HardwareBufferMemory>>, PropertiesError> {
    let description = HardwareBufferDescription::of(buffer);
    if let Some(rejection) = description.rejection() {
        return Ok(Sampling::Rejected(rejection));
    }
    let HardwareBufferDescription { size, alpha, .. } = description;
    let memory = HardwareBufferMemory::read(importer, buffer, size, alpha)?;
    Ok(memory.map_or(
        Sampling::Rejected(StreamRejection::UnusableBuffer),
        |memory| importer.sampling(memory),
    ))
}
