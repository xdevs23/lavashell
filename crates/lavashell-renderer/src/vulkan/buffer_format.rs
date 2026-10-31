// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests;

use super::SkiaFormat;
use crate::paint::{StreamRejection, UnsampledFormat};
use ash::vk;
use lavashell_vulkan::ImageFormat;
use skia_safe::gpu::vk as skvk;

/// The format that the driver reports for foreign memory: its Vulkan format, its external format, the features of
/// that format, and the YCbCr conversion that the driver suggests for sampling it.
#[derive(Clone, Copy, Debug)]
pub struct ForeignFormat {
    pub format: vk::Format,
    pub external_format: u64,
    pub features: vk::FormatFeatureFlags,
    pub components: vk::ComponentMapping,
    pub model: vk::SamplerYcbcrModelConversion,
    pub range: vk::SamplerYcbcrRange,
    /// The chroma offsets along x and along y.
    pub chroma_offsets: [vk::ChromaLocation; 2],
}

impl ForeignFormat {
    /// The format of the image that imports memory of this format. Memory in `R8G8B8A8_UNORM` is imported in that
    /// Vulkan format, memory in any other format in its external format.
    #[must_use]
    pub fn image_format(&self) -> ImageFormat {
        if self.format == SkiaFormat::RGBA8.vulkan {
            ImageFormat::Known(self.format)
        } else {
            ImageFormat::External
        }
    }
}

// Skia wraps an image in a Vulkan format only when the image also has both transfer usages, as the images created by
// Skia's own import of foreign memory have.
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

/// The format in which foreign memory is imported. Memory imported in a Vulkan format, as `ForeignFormat::image_format`
/// returns it, is sampled directly. Memory in any other format is sampled in its external format through a YCbCr
/// conversion. That needs `ycbcr_conversion`, which is true when the device supports YCbCr conversion. Without that
/// support, the memory is rejected.
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

/// The Skia YCbCr conversion for the external format of `format`, with the model, range, chroma offsets and component
/// mapping reported by the driver. Vulkan calls the model, range and offsets suggested, because an application may
/// sample with others. The renderer uses them unchanged. Returns `None` when the external format is 0 or when one of
/// these values has no Skia equivalent.
fn skia_conversion(format: &ForeignFormat) -> Option<skvk::YcbcrConversionInfo> {
    // Vulkan reports an external format other than 0 for all importable memory in an external format. A format with
    // an external format of 0 therefore describes no memory that can be imported.
    if format.external_format == 0 {
        return None;
    }
    let linear = vk::FormatFeatureFlags::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER;
    let chroma_filter = if format.features.contains(linear) {
        skvk::Filter::LINEAR
    } else {
        skvk::Filter::NEAREST
    };
    let components = format.components;
    let [x_chroma_offset, y_chroma_offset] = format.chroma_offsets;
    Some(skvk::YcbcrConversionInfo::new_with_external_format(
        format.external_format,
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
