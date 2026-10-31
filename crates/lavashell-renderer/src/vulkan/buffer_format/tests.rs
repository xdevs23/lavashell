// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;

const SAMPLED: vk::FormatFeatureFlags = vk::FormatFeatureFlags::SAMPLED_IMAGE;

/// A foreign format of `format` in `external_format` with `features`, and the defaults of Vulkan for the rest.
fn foreign_format(
    format: vk::Format,
    external_format: u64,
    features: vk::FormatFeatureFlags,
) -> ForeignFormat {
    ForeignFormat {
        format,
        external_format,
        features,
        components: vk::ComponentMapping::default(),
        model: vk::SamplerYcbcrModelConversion::default(),
        range: vk::SamplerYcbcrRange::default(),
        chroma_offsets: [vk::ChromaLocation::default(); 2],
    }
}

/// The YCbCr conversion of the import format of `format` on a device with YCbCr conversion. Asserts first that the
/// import creates its image in the external format and with sampled usage only, as Vulkan requires for an image in an
/// external format.
fn external(format: &ForeignFormat) -> Result<skvk::YcbcrConversionInfo, TestError> {
    assert_eq!(format.image_format(), ImageFormat::External);
    let import = import_format(format, true).map_err(TestError::Rejected)?;
    assert_eq!(import.skia_format(), skvk::Format::UNDEFINED);
    assert_eq!(import.usage(), vk::ImageUsageFlags::SAMPLED);
    import
        .conversion()
        .ok_or(TestError::Missing("YCbCr conversion"))
}

#[test]
fn memory_of_the_rgba_format_imports_without_a_conversion_on_every_device() -> Result<(), TestError>
{
    let rgba = foreign_format(vk::Format::R8G8B8A8_UNORM, 1, SAMPLED);
    assert_eq!(
        rgba.image_format(),
        ImageFormat::Known(vk::Format::R8G8B8A8_UNORM)
    );
    let transfer = vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;
    for ycbcr_conversion in [true, false] {
        let format = import_format(&rgba, ycbcr_conversion).map_err(TestError::Rejected)?;
        assert_eq!(format.skia_format(), skvk::Format::R8G8B8A8_UNORM);
        assert_eq!(format.usage(), vk::ImageUsageFlags::SAMPLED | transfer);
        assert!(format.conversion().is_none());
    }
    Ok(())
}

#[test]
fn memory_in_ycbcr_imports_with_the_conversion_suggested_by_the_driver() -> Result<(), TestError> {
    let features = SAMPLED
        | vk::FormatFeatureFlags::MIDPOINT_CHROMA_SAMPLES
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
    let ycbcr = ForeignFormat {
        components: vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::ONE,
        },
        model: vk::SamplerYcbcrModelConversion::YCBCR_709,
        range: vk::SamplerYcbcrRange::ITU_NARROW,
        chroma_offsets: [vk::ChromaLocation::MIDPOINT; 2],
        ..foreign_format(vk::Format::G8_B8R8_2PLANE_420_UNORM, 0x3231_564e, features)
    };
    let conversion = external(&ycbcr)?;
    assert_eq!(conversion.format(), skvk::Format::UNDEFINED);
    assert_eq!(conversion.external_format(), 0x3231_564e);
    assert_eq!(
        conversion.model(),
        skvk::SamplerYcbcrModelConversion::YCBCR_709
    );
    assert_eq!(conversion.range(), skvk::SamplerYcbcrRange::ITU_NARROW);
    assert_eq!(conversion.x_chroma_offset(), skvk::ChromaLocation::MIDPOINT);
    assert_eq!(conversion.y_chroma_offset(), skvk::ChromaLocation::MIDPOINT);
    assert_eq!(conversion.chroma_filter(), skvk::Filter::LINEAR);
    assert_eq!(conversion.force_explicit_reconstruction(), vk::FALSE);
    let components = conversion.components();
    let identity = skvk::ComponentSwizzle::VK_COMPONENT_SWIZZLE_IDENTITY;
    assert_eq!([components.r, components.g, components.b], [identity; 3]);
    assert_eq!(
        components.a,
        skvk::ComponentSwizzle::VK_COMPONENT_SWIZZLE_ONE
    );
    assert!(conversion.supports_linear_filter());
    Ok(())
}

#[test]
fn memory_without_a_vulkan_format_imports_with_the_conversion_of_its_external_format()
-> Result<(), TestError> {
    let features = SAMPLED | vk::FormatFeatureFlags::COSITED_CHROMA_SAMPLES;
    let undefined = ForeignFormat {
        model: vk::SamplerYcbcrModelConversion::RGB_IDENTITY,
        range: vk::SamplerYcbcrRange::ITU_FULL,
        chroma_offsets: [vk::ChromaLocation::COSITED_EVEN; 2],
        ..foreign_format(vk::Format::UNDEFINED, 0x22, features)
    };
    let conversion = external(&undefined)?;
    assert_eq!(conversion.external_format(), 0x22);
    assert_eq!(
        conversion.model(),
        skvk::SamplerYcbcrModelConversion::RGB_IDENTITY
    );
    assert_eq!(conversion.range(), skvk::SamplerYcbcrRange::ITU_FULL);
    assert_eq!(
        conversion.x_chroma_offset(),
        skvk::ChromaLocation::COSITED_EVEN
    );
    assert_eq!(
        conversion.y_chroma_offset(),
        skvk::ChromaLocation::COSITED_EVEN
    );
    assert_eq!(conversion.chroma_filter(), skvk::Filter::NEAREST);
    assert!(!conversion.supports_linear_filter());
    Ok(())
}

#[test]
fn a_device_without_ycbcr_conversion_rejects_the_memory_that_needs_one() {
    let ycbcr = ForeignFormat {
        model: vk::SamplerYcbcrModelConversion::YCBCR_601,
        range: vk::SamplerYcbcrRange::ITU_NARROW,
        ..foreign_format(vk::Format::G8_B8_R8_3PLANE_420_UNORM, 0x3231_5659, SAMPLED)
    };
    let undefined = foreign_format(vk::Format::UNDEFINED, 0x22, SAMPLED);
    let half_float = foreign_format(vk::Format::R16G16B16A16_SFLOAT, 0x16, SAMPLED);
    for format in [ycbcr, undefined, half_float] {
        let rejection = import_format(&format, false).err();
        assert_eq!(rejection, Some(NEEDS_YCBCR_CONVERSION));
    }
}

#[test]
fn memory_of_another_vulkan_format_in_rgb_imports_with_the_conversion_of_its_external_format()
-> Result<(), TestError> {
    let half_float = foreign_format(vk::Format::R16G16B16A16_SFLOAT, 0x16, SAMPLED);
    let conversion = external(&half_float)?;
    assert_eq!(conversion.external_format(), 0x16);
    assert_eq!(
        conversion.model(),
        skvk::SamplerYcbcrModelConversion::RGB_IDENTITY
    );
    Ok(())
}

#[test]
fn memory_described_with_values_unknown_to_skia_is_unusable() {
    let unknown_model = ForeignFormat {
        model: vk::SamplerYcbcrModelConversion::from_raw(77),
        ..foreign_format(vk::Format::UNDEFINED, 0x22, SAMPLED)
    };
    let unknown_swizzle = ForeignFormat {
        components: vk::ComponentMapping {
            r: vk::ComponentSwizzle::from_raw(9),
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        },
        ..foreign_format(vk::Format::UNDEFINED, 0x22, SAMPLED)
    };
    let without_external_format = foreign_format(vk::Format::UNDEFINED, 0, SAMPLED);
    for format in [unknown_model, unknown_swizzle, without_external_format] {
        let rejection = import_format(&format, true).err();
        assert_eq!(rejection, Some(StreamRejection::UnusableBuffer));
    }
}
