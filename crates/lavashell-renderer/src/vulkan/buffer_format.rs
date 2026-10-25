// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests;

use super::SkiaFormat;
use crate::paint::{StreamRejection, UnsampledFormat};
use ash::vk;
use skia_safe::gpu::vk as skvk;

// Skia wraps an image in a Vulkan format only when the image also has both transfer usages, as the images created by
// the hardware buffer import inside Skia have.
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
    pub fn vulkan_format(self) -> vk::Format {
        match self {
            Self::Rgba => SkiaFormat::RGBA8.vulkan,
            Self::External(_) => vk::Format::UNDEFINED,
        }
    }

    pub fn skia_format(self) -> skvk::Format {
        match self {
            Self::Rgba => SkiaFormat::RGBA8.skia,
            Self::External(_) => skvk::Format::UNDEFINED,
        }
    }

    pub fn usage(self) -> vk::ImageUsageFlags {
        match self {
            Self::Rgba => RGBA_USAGE,
            // Vulkan allows an image in an external format no usage other than sampling through a YCbCr conversion.
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

/// The format in which an AOSP hardware buffer is imported, chosen from the format properties that Vulkan reports for
/// the buffer. A buffer in the Vulkan format `R8G8B8A8_UNORM` is sampled directly. A buffer in any other format is
/// sampled in its external format through a YCbCr conversion. That needs `ycbcr_conversion`, which is true when the
/// device supports YCbCr conversion. Without that support, the buffer is rejected.
pub(super) fn import_format(
    buffer: &vk::AndroidHardwareBufferFormatPropertiesANDROID,
    ycbcr_conversion: bool,
) -> Result<ImportFormat, StreamRejection> {
    if buffer.format == SkiaFormat::RGBA8.vulkan {
        return Ok(ImportFormat::Rgba);
    }
    if !ycbcr_conversion {
        return Err(NEEDS_YCBCR_CONVERSION);
    }
    let conversion = skia_conversion(buffer).ok_or(StreamRejection::UnusableBuffer)?;
    Ok(ImportFormat::External(conversion))
}

/// The Skia YCbCr conversion for the external format of `buffer`, with the model, range, chroma offsets and component
/// mapping reported by the driver. Vulkan calls the model, range and offsets suggested, because an application may
/// sample with others. The renderer uses them unchanged. Returns `None` when the external format is 0 or when one of
/// these values has no Skia equivalent.
fn skia_conversion(
    buffer: &vk::AndroidHardwareBufferFormatPropertiesANDROID,
) -> Option<skvk::YcbcrConversionInfo> {
    // Vulkan reports an external format other than 0 for every AOSP hardware buffer. Properties with an external
    // format of 0 therefore describe no buffer that can be imported.
    if buffer.external_format == 0 {
        return None;
    }
    let linear = vk::FormatFeatureFlags::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER;
    let chroma_filter = if buffer.format_features.contains(linear) {
        skvk::Filter::LINEAR
    } else {
        skvk::Filter::NEAREST
    };
    let components = buffer.sampler_ycbcr_conversion_components;
    Some(skvk::YcbcrConversionInfo::new_with_external_format(
        buffer.external_format,
        skia_model(buffer.suggested_ycbcr_model)?,
        skia_range(buffer.suggested_ycbcr_range)?,
        skia_chroma_location(buffer.suggested_x_chroma_offset)?,
        skia_chroma_location(buffer.suggested_y_chroma_offset)?,
        chroma_filter,
        vk::FALSE,
        skvk::ComponentMapping {
            r: skia_swizzle(components.r)?,
            g: skia_swizzle(components.g)?,
            b: skia_swizzle(components.b)?,
            a: skia_swizzle(components.a)?,
        },
        buffer.format_features.as_raw(),
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
