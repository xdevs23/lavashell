// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan::SkiaFormat;
use ash::vk;
use lavashell_core::prelude::*;
use skia_safe::ColorType;

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
}
