// SPDX-License-Identifier: GPL-3.0-only

use crate::platform::ndk::hardware_buffer::{HardwareBuffer, HardwareBufferUsage};
use crate::platform::ndk::hardware_buffer_format::HardwareBufferFormat;
use crate::platform::{
    ForeignImporter, HardwareBufferMemory, PropertiesError, SampledMemory, Sampling,
};
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{ForeignAlpha, StreamRejection};
use std::collections::BTreeSet;

struct HardwareBufferDescription {
    size: PixelSize,
    layers: u32,
    usage: BTreeSet<BufferFlag>,
    alpha: ForeignAlpha,
}

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

    fn rejection(&self) -> Option<StreamRejection> {
        let usage = &self.usage;
        let sampled = self.layers == 1 && usage.contains(&BufferFlag::GpuSampled);
        (!sampled
            || usage.contains(&BufferFlag::Protected)
            || usage.contains(&BufferFlag::MipmapComplete))
        .then_some(StreamRejection::UnusableBuffer)
    }
}

pub(crate) fn read_sampling(
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
