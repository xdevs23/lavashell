// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;

const SAMPLED: vk::FormatFeatureFlags = vk::FormatFeatureFlags::SAMPLED_IMAGE;

/// The YCbCr conversion of an import format for a buffer in an external format. Asserts first that the import
/// creates its image with an undefined format and with sampled usage only, as Vulkan requires for an image in an
/// external format.
fn external(
    format: Result<ImportFormat, StreamRejection>,
) -> Result<skvk::YcbcrConversionInfo, TestError> {
    let format = format.map_err(TestError::Rejected)?;
    assert_eq!(format.vulkan_format(), vk::Format::UNDEFINED);
    assert_eq!(format.skia_format(), skvk::Format::UNDEFINED);
    assert_eq!(format.usage(), vk::ImageUsageFlags::SAMPLED);
    format
        .conversion()
        .ok_or(TestError::Missing("YCbCr conversion"))
}

#[test]
fn a_buffer_of_the_rgba_format_imports_without_a_conversion_on_every_device()
-> Result<(), TestError> {
    let buffer = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::R8G8B8A8_UNORM)
        .external_format(1)
        .format_features(SAMPLED);
    let transfer = vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;
    for ycbcr_conversion in [true, false] {
        let format = import_format(&buffer, ycbcr_conversion).map_err(TestError::Rejected)?;
        assert_eq!(format.vulkan_format(), vk::Format::R8G8B8A8_UNORM);
        assert_eq!(format.skia_format(), skvk::Format::R8G8B8A8_UNORM);
        assert_eq!(format.usage(), vk::ImageUsageFlags::SAMPLED | transfer);
        assert!(format.conversion().is_none());
    }
    Ok(())
}

#[test]
fn a_buffer_in_ycbcr_imports_with_the_conversion_suggested_by_the_driver() -> Result<(), TestError>
{
    let features = SAMPLED
        | vk::FormatFeatureFlags::MIDPOINT_CHROMA_SAMPLES
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_YCBCR_CONVERSION_LINEAR_FILTER
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
    let buffer = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
        .external_format(0x3231_564e)
        .format_features(features)
        .sampler_ycbcr_conversion_components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::ONE,
        })
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::YCBCR_709)
        .suggested_ycbcr_range(vk::SamplerYcbcrRange::ITU_NARROW)
        .suggested_x_chroma_offset(vk::ChromaLocation::MIDPOINT)
        .suggested_y_chroma_offset(vk::ChromaLocation::MIDPOINT);
    let conversion = external(import_format(&buffer, true))?;
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
fn a_buffer_without_a_vulkan_format_imports_with_the_conversion_of_its_external_format()
-> Result<(), TestError> {
    let buffer = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::UNDEFINED)
        .external_format(0x22)
        .format_features(SAMPLED | vk::FormatFeatureFlags::COSITED_CHROMA_SAMPLES)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::RGB_IDENTITY)
        .suggested_ycbcr_range(vk::SamplerYcbcrRange::ITU_FULL)
        .suggested_x_chroma_offset(vk::ChromaLocation::COSITED_EVEN)
        .suggested_y_chroma_offset(vk::ChromaLocation::COSITED_EVEN);
    let conversion = external(import_format(&buffer, true))?;
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
fn a_device_without_ycbcr_conversion_rejects_the_buffers_that_need_one() {
    let ycbcr = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::G8_B8_R8_3PLANE_420_UNORM)
        .external_format(0x3231_5659)
        .format_features(SAMPLED)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::YCBCR_601)
        .suggested_ycbcr_range(vk::SamplerYcbcrRange::ITU_NARROW);
    let undefined = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::UNDEFINED)
        .external_format(0x22)
        .format_features(SAMPLED)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::RGB_IDENTITY);
    let half_float = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::R16G16B16A16_SFLOAT)
        .external_format(0x16)
        .format_features(SAMPLED)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::RGB_IDENTITY);
    for buffer in [ycbcr, undefined, half_float] {
        let rejection = import_format(&buffer, false).err();
        assert_eq!(rejection, Some(NEEDS_YCBCR_CONVERSION));
    }
}

#[test]
fn a_buffer_of_another_vulkan_format_in_rgb_imports_with_the_conversion_of_its_external_format()
-> Result<(), TestError> {
    let buffer = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::R16G16B16A16_SFLOAT)
        .external_format(0x16)
        .format_features(SAMPLED)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::RGB_IDENTITY)
        .suggested_ycbcr_range(vk::SamplerYcbcrRange::ITU_FULL);
    let conversion = external(import_format(&buffer, true))?;
    assert_eq!(conversion.external_format(), 0x16);
    assert_eq!(
        conversion.model(),
        skvk::SamplerYcbcrModelConversion::RGB_IDENTITY
    );
    Ok(())
}

#[test]
fn a_buffer_described_with_values_unknown_to_skia_is_unusable() {
    let unknown_model = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::UNDEFINED)
        .external_format(0x22)
        .format_features(SAMPLED)
        .suggested_ycbcr_model(vk::SamplerYcbcrModelConversion::from_raw(77));
    let unknown_swizzle = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::UNDEFINED)
        .external_format(0x22)
        .format_features(SAMPLED)
        .sampler_ycbcr_conversion_components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::from_raw(9),
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        });
    let without_external_format = vk::AndroidHardwareBufferFormatPropertiesANDROID::default()
        .format(vk::Format::UNDEFINED)
        .external_format(0)
        .format_features(SAMPLED);
    for buffer in [unknown_model, unknown_swizzle, without_external_format] {
        let rejection = import_format(&buffer, true).err();
        assert_eq!(rejection, Some(StreamRejection::UnusableBuffer));
    }
}
