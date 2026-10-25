// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan::SkiaFormat;
use ash::vk;
use lavashell_core::prelude::*;
use skia_safe::{ColorSpace, ColorType};
use skia_safe::{named_primaries::CicpId as P, named_transfer_fn::CicpId as T};

/// The color space of the surfaces in which the renderer paints and blends: linear Rec.2020. `None` when Skia
/// cannot create it.
#[must_use]
pub fn working_color_space() -> Option<ColorSpace> {
    ColorSpace::new_cicp(P::Rec2020, T::Linear)
}

/// The Skia color space for the output `profile`, or `None` when Skia cannot create it.
#[must_use]
pub fn output_color_space(profile: OutputProfile) -> Option<ColorSpace> {
    match profile {
        OutputProfile::Srgb => Some(ColorSpace::new_srgb()),
        OutputProfile::DisplayP3 => ColorSpace::new_cicp(P::SMPTE_EG_432_1, T::SRGB),
        OutputProfile::Rec2020 => ColorSpace::new_cicp(P::Rec2020, T::Linear),
        OutputProfile::Hdr10 => ColorSpace::new_cicp(P::Rec2020, T::PQ),
        OutputProfile::ScRgb => Some(ColorSpace::new_srgb_linear()),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Selection {
    pub format: vk::SurfaceFormatKHR,
    pub color: ColorType,
    pub info: OutputInfo,
}

fn color_space(profile: OutputProfile) -> vk::ColorSpaceKHR {
    match profile {
        OutputProfile::Srgb => vk::ColorSpaceKHR::SRGB_NONLINEAR,
        OutputProfile::DisplayP3 => vk::ColorSpaceKHR::DISPLAY_P3_NONLINEAR_EXT,
        OutputProfile::Rec2020 => vk::ColorSpaceKHR::BT2020_LINEAR_EXT,
        OutputProfile::Hdr10 => vk::ColorSpaceKHR::HDR10_ST2084_EXT,
        OutputProfile::ScRgb => vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT,
    }
}

/// The first format for the precision of `request` that the surface offers in `formats` with the color space of
/// the requested profile. When the surface offers none, or the request pairs a profile with a precision that cannot
/// show it, the result is the selection for the default request, SDR sRGB with 8 bits, marked as a fallback. `None`
/// when the surface offers no format for the default request either.
pub(crate) fn select(
    formats: &[vk::SurfaceFormatKHR],
    request: OutputRequest,
) -> Option<Selection> {
    let candidates: &[(SkiaFormat, u8, u8, bool)] = match request.precision {
        Precision::Eight => &[
            (SkiaFormat::BGRA8, 8, 8, false),
            (SkiaFormat::RGBA8, 8, 8, false),
        ],
        Precision::Ten => &[
            (SkiaFormat::RGB10_A2, 10, 2, false),
            (SkiaFormat::BGR10_A2, 10, 2, false),
        ],
        Precision::Twelve => &[(SkiaFormat::RGBA16, 16, 16, false)],
        Precision::Float16 => &[(SkiaFormat::RGBA16_FLOAT, 16, 16, true)],
    };
    // HDR10 needs at least ten bits per color channel. scRGB needs half floats for its values outside 0 to 1. Such a
    // request with a narrower format falls back to sRGB, so that no surface is labelled with a profile it cannot show.
    let coherent = (request.profile != OutputProfile::Hdr10
        || request.precision != Precision::Eight)
        && (request.profile != OutputProfile::ScRgb || request.precision == Precision::Float16);
    if coherent {
        for &(format, bits, alpha, float) in candidates {
            if formats.iter().any(|v| {
                (v.format == format.vulkan || v.format == vk::Format::UNDEFINED)
                    && v.color_space == color_space(request.profile)
            }) {
                return Some(Selection {
                    format: vk::SurfaceFormatKHR {
                        format: format.vulkan,
                        color_space: color_space(request.profile),
                    },
                    color: format.color_type,
                    info: OutputInfo {
                        requested: request,
                        profile: request.profile,
                        color_bits: bits,
                        alpha_bits: alpha,
                        floating_point: float,
                        fallback: false,
                    },
                });
            }
        }
    }
    if request != OutputRequest::default() {
        let mut fallback = select(formats, OutputRequest::default())?;
        fallback.info.requested = request;
        fallback.info.fallback = true;
        return Some(fallback);
    }
    None
}

/// The relative precision of a half float: one step of its ten fraction bits.
pub(crate) const HALF_EPSILON: f64 = 1.0 / 1024.0;

/// The step between two neighboring values of the coarsest color channel of `color_type`, or None for a color type
/// without channels.
pub(crate) fn channel_step(color_type: ColorType) -> Option<f64> {
    let unorm = |bits: u32| 1.0 / f64::from(u16::MAX >> (u16::BITS - bits));
    Some(match color_type {
        ColorType::Unknown => return None,
        ColorType::ARGB4444 => unorm(4),
        ColorType::RGB565 => unorm(5),
        ColorType::Alpha8
        | ColorType::RGBA8888
        | ColorType::RGB888x
        | ColorType::BGRA8888
        | ColorType::Gray8
        | ColorType::R8G8UNorm
        | ColorType::SRGBA8888
        | ColorType::R8UNorm => unorm(8),
        ColorType::RGBA1010102
        | ColorType::BGRA1010102
        | ColorType::RGB101010x
        | ColorType::BGR101010x
        | ColorType::RGBA10x6 => unorm(10),
        // The extended range formats spread their 10-bit values over about two units, which gives 510 steps per unit.
        ColorType::BGR101010xXR | ColorType::BGRA10101010XR => 1.0 / 510.0,
        ColorType::A16UNorm
        | ColorType::R16UNorm
        | ColorType::R16G16UNorm
        | ColorType::R16G16B16A16UNorm => unorm(16),
        ColorType::RGBAF16Norm
        | ColorType::RGBAF16
        | ColorType::RGBF16F16F16x
        | ColorType::A16Float
        | ColorType::R16Float
        | ColorType::R16G16Float => HALF_EPSILON,
        ColorType::RGBAF32 => f64::from(f32::EPSILON),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::test_error::TestError;

    fn pair(f: vk::Format, c: vk::ColorSpaceKHR) -> vk::SurfaceFormatKHR {
        vk::SurfaceFormatKHR {
            format: f,
            color_space: c,
        }
    }

    #[test]
    fn selection_never_pairs_an_unadvertised_format_with_a_profile() -> Result<(), TestError> {
        let formats = [
            pair(
                vk::Format::B8G8R8A8_UNORM,
                vk::ColorSpaceKHR::SRGB_NONLINEAR,
            ),
            pair(
                vk::Format::A2B10G10R10_UNORM_PACK32,
                vk::ColorSpaceKHR::HDR10_ST2084_EXT,
            ),
        ];
        let hdr = select(
            &formats,
            OutputRequest::new(OutputProfile::Hdr10, Precision::Ten),
        )
        .ok_or(Error::NoSurfaceFormat)?;
        assert_eq!(hdr.info.color_bits, 10);
        assert!(!hdr.info.fallback);
        let p3 = select(
            &formats,
            OutputRequest::new(OutputProfile::DisplayP3, Precision::Ten),
        )
        .ok_or(Error::NoSurfaceFormat)?;
        assert!(p3.info.fallback);
        assert_eq!(p3.info.profile, OutputProfile::Srgb);
        assert!(select(&formats[1..], OutputRequest::default()).is_none());
        Ok(())
    }

    #[test]
    fn twelve_bit_request_reports_actual_sixteen_bit_storage() -> Result<(), TestError> {
        let s = select(
            &[pair(
                vk::Format::R16G16B16A16_UNORM,
                vk::ColorSpaceKHR::SRGB_NONLINEAR,
            )],
            OutputRequest::new(OutputProfile::Srgb, Precision::Twelve),
        )
        .ok_or(Error::NoSurfaceFormat)?;
        assert_eq!(s.info.color_bits, 16);
        assert!(!s.info.floating_point);
        Ok(())
    }

    #[test]
    fn profiles_create_color_spaces() {
        for p in [
            OutputProfile::Srgb,
            OutputProfile::DisplayP3,
            OutputProfile::Rec2020,
            OutputProfile::Hdr10,
            OutputProfile::ScRgb,
        ] {
            assert!(output_color_space(p).is_some());
        }
    }
}
