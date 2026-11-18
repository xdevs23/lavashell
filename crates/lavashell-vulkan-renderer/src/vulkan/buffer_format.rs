// SPDX-License-Identifier: GPL-3.0-only
use super::SkiaFormat;
use ash::vk;
use lavashell_renderer::{StreamRejection, UnsampledFormat};
use lavashell_vulkan::ImageFormat;
use skia_safe::gpu::vk as skvk;
use std::num::NonZeroU64;

#[derive(Clone, Copy, Debug)]
pub struct ForeignFormat {
    pub format: vk::Format,
    pub external_format: Option<NonZeroU64>,
    pub features: vk::FormatFeatureFlags,
    pub components: vk::ComponentMapping,
    pub model: vk::SamplerYcbcrModelConversion,
    pub range: vk::SamplerYcbcrRange,
    pub chroma_offsets: [vk::ChromaLocation; 2],
}

impl ForeignFormat {
    #[must_use]
    pub fn image_format(&self) -> ImageFormat {
        if self.format == SkiaFormat::RGBA8.vulkan {
            ImageFormat::Known(self.format)
        } else {
            ImageFormat::External
        }
    }
}

const RGBA_USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::from_raw(
    vk::ImageUsageFlags::SAMPLED.as_raw()
        | vk::ImageUsageFlags::TRANSFER_SRC.as_raw()
        | vk::ImageUsageFlags::TRANSFER_DST.as_raw(),
);

pub(super) const NEEDS_YCBCR_CONVERSION: StreamRejection =
    StreamRejection::BufferFormat(UnsampledFormat::NeedsYcbcrConversion);

#[derive(Clone, Copy)]
pub(super) enum ImportFormat {
    Rgba,
    External(skvk::YcbcrConversionInfo),
}

impl ImportFormat {
    pub fn skia_format(self) -> skvk::Format {
        match self {
            Self::Rgba => SkiaFormat::RGBA8.skia,
            Self::External(_) => skvk::Format::UNDEFINED,
        }
    }

    pub fn usage(self) -> vk::ImageUsageFlags {
        match self {
            Self::Rgba => RGBA_USAGE,
            Self::External(_) => vk::ImageUsageFlags::SAMPLED,
        }
    }

    pub fn conversion(self) -> Option<skvk::YcbcrConversionInfo> {
        match self {
            Self::Rgba => None,
            Self::External(conversion) => Some(conversion),
        }
    }
}

pub(super) fn import_format(
    format: &ForeignFormat,
    ycbcr_conversion: bool,
) -> Result<ImportFormat, StreamRejection> {
    if let ImageFormat::Known(_) = format.image_format() {
        return Ok(ImportFormat::Rgba);
    }
    if !ycbcr_conversion {
        return Err(NEEDS_YCBCR_CONVERSION);
    }
    let conversion = skia_conversion(format).ok_or(StreamRejection::UnusableBuffer)?;
    Ok(ImportFormat::External(conversion))
}

fn skia_conversion(format: &ForeignFormat) -> Option<skvk::YcbcrConversionInfo> {
    let external_format = format.external_format?;
    let linear = vk::FormatFeatureFlags::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER;
    let chroma_filter = if format.features.contains(linear) {
        skvk::Filter::LINEAR
    } else {
        skvk::Filter::NEAREST
    };
    let components = format.components;
    let [x_chroma_offset, y_chroma_offset] = format.chroma_offsets;
    Some(skvk::YcbcrConversionInfo::new_with_external_format(
        external_format.get(),
        skia_model(format.model)?,
        skia_range(format.range)?,
        skia_chroma_location(x_chroma_offset)?,
        skia_chroma_location(y_chroma_offset)?,
        chroma_filter,
        vk::FALSE,
        skvk::ComponentMapping {
            r: skia_swizzle(components.r)?,
            g: skia_swizzle(components.g)?,
            b: skia_swizzle(components.b)?,
            a: skia_swizzle(components.a)?,
        },
        format.features.as_raw(),
    ))
}

fn skia_model(model: vk::SamplerYcbcrModelConversion) -> Option<skvk::SamplerYcbcrModelConversion> {
    use skvk::SamplerYcbcrModelConversion as Skia;
    use vk::SamplerYcbcrModelConversion as Vulkan;
    Some(match model {
        Vulkan::RGB_IDENTITY => Skia::RGB_IDENTITY,
        Vulkan::YCBCR_IDENTITY => Skia::YCBCR_IDENTITY,
        Vulkan::YCBCR_709 => Skia::YCBCR_709,
        Vulkan::YCBCR_601 => Skia::YCBCR_601,
        Vulkan::YCBCR_2020 => Skia::YCBCR_2020,
        _ => return None,
    })
}

fn skia_range(range: vk::SamplerYcbcrRange) -> Option<skvk::SamplerYcbcrRange> {
    match range {
        vk::SamplerYcbcrRange::ITU_FULL => Some(skvk::SamplerYcbcrRange::ITU_FULL),
        vk::SamplerYcbcrRange::ITU_NARROW => Some(skvk::SamplerYcbcrRange::ITU_NARROW),
        _ => None,
    }
}

fn skia_chroma_location(location: vk::ChromaLocation) -> Option<skvk::ChromaLocation> {
    match location {
        vk::ChromaLocation::COSITED_EVEN => Some(skvk::ChromaLocation::COSITED_EVEN),
        vk::ChromaLocation::MIDPOINT => Some(skvk::ChromaLocation::MIDPOINT),
        _ => None,
    }
}

fn skia_swizzle(swizzle: vk::ComponentSwizzle) -> Option<skvk::ComponentSwizzle> {
    use skvk::ComponentSwizzle as Skia;
    use vk::ComponentSwizzle as Vulkan;
    Some(match swizzle {
        Vulkan::IDENTITY => Skia::VK_COMPONENT_SWIZZLE_IDENTITY,
        Vulkan::ZERO => Skia::VK_COMPONENT_SWIZZLE_ZERO,
        Vulkan::ONE => Skia::VK_COMPONENT_SWIZZLE_ONE,
        Vulkan::R => Skia::VK_COMPONENT_SWIZZLE_R,
        Vulkan::G => Skia::VK_COMPONENT_SWIZZLE_G,
        Vulkan::B => Skia::VK_COMPONENT_SWIZZLE_B,
        Vulkan::A => Skia::VK_COMPONENT_SWIZZLE_A,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
