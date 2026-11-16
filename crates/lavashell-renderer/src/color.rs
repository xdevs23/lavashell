// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::vulkan::SkiaFormat;
use ash::vk;
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, ISize, ImageInfo};
use skia_safe::{named_primaries::CicpId as P, named_transfer_fn::CicpId as T};

// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/opts/SkRasterPipeline_opts.h
const EXTENDED_RANGE_CODES_PER_UNIT: f64 = 510.0;

pub fn working_color_space() -> Result<ColorSpace, Error> {
    ColorSpace::new_cicp(P::Rec2020, T::Linear).ok_or(Error::WorkingColorSpace)
}

pub(crate) fn working_image_info(size: impl Into<ISize>) -> Result<ImageInfo, Error> {
    Ok(ImageInfo::new(
        size,
        ColorType::RGBAF16,
        AlphaType::Premul,
        working_color_space()?,
    ))
}

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

struct Candidate {
    format: SkiaFormat,
    color_bits: u8,
    alpha_bits: u8,
    floating_point: bool,
}

impl Candidate {
    const fn unorm(format: SkiaFormat, color_bits: u8, alpha_bits: u8) -> Self {
        Self {
            format,
            color_bits,
            alpha_bits,
            floating_point: false,
        }
    }
}

pub(crate) fn select(
    formats: &[vk::SurfaceFormatKHR],
    request: OutputRequest,
) -> Option<Selection> {
    let candidates: &[Candidate] = match request.precision {
        Precision::Eight => &[
            Candidate::unorm(SkiaFormat::BGRA8, 8, 8),
            Candidate::unorm(SkiaFormat::RGBA8, 8, 8),
        ],
        Precision::Ten => &[
            Candidate::unorm(SkiaFormat::RGB10_A2, 10, 2),
            Candidate::unorm(SkiaFormat::BGR10_A2, 10, 2),
        ],
        Precision::Twelve => &[Candidate::unorm(SkiaFormat::RGBA16, 16, 16)],
        Precision::Float16 => &[Candidate {
            format: SkiaFormat::RGBA16_FLOAT,
            color_bits: 16,
            alpha_bits: 16,
            floating_point: true,
        }],
    };
    let coherent = (request.profile != OutputProfile::Hdr10
        || request.precision != Precision::Eight)
        && (request.profile != OutputProfile::ScRgb || request.precision == Precision::Float16);
    if coherent {
        for candidate in candidates {
            let format = candidate.format;
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
                        color_bits: candidate.color_bits,
                        alpha_bits: candidate.alpha_bits,
                        floating_point: candidate.floating_point,
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

pub(crate) const HALF_EPSILON: f64 = 1.0 / 1024.0;

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
        ColorType::BGR101010xXR | ColorType::BGRA10101010XR => 1.0 / EXTENDED_RANGE_CODES_PER_UNIT,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChannelRange {
    Unit,
    Extended,
}

impl ChannelRange {
    pub fn of(color_type: ColorType) -> Self {
        match color_type {
            ColorType::Unknown
            | ColorType::ARGB4444
            | ColorType::RGB565
            | ColorType::Alpha8
            | ColorType::RGBA8888
            | ColorType::RGB888x
            | ColorType::BGRA8888
            | ColorType::Gray8
            | ColorType::R8G8UNorm
            | ColorType::SRGBA8888
            | ColorType::R8UNorm
            | ColorType::RGBA1010102
            | ColorType::BGRA1010102
            | ColorType::RGB101010x
            | ColorType::BGR101010x
            | ColorType::RGBA10x6
            | ColorType::A16UNorm
            | ColorType::R16UNorm
            | ColorType::R16G16UNorm
            | ColorType::R16G16B16A16UNorm
            | ColorType::RGBAF16Norm => Self::Unit,
            ColorType::BGR101010xXR
            | ColorType::BGRA10101010XR
            | ColorType::RGBAF16
            | ColorType::RGBF16F16F16x
            | ColorType::A16Float
            | ColorType::R16Float
            | ColorType::R16G16Float
            | ColorType::RGBAF32 => Self::Extended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
