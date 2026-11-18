// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, ISize, ImageInfo};
use skia_safe::{named_primaries::CicpId as P, named_transfer_fn::CicpId as T};

// https://github.com/rust-skia/skia/blob/m153-0.101.2/src/opts/SkRasterPipeline_opts.h
const EXTENDED_RANGE_CODES_PER_UNIT: f64 = 510.0;

pub fn working_color_space() -> Result<ColorSpace, Error> {
    ColorSpace::new_cicp(P::Rec2020, T::Linear).ok_or(Error::WorkingColorSpace)
}

pub fn working_image_info(size: impl Into<ISize>) -> Result<ImageInfo, Error> {
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

    #[test]
    fn profiles_create_color_spaces() {
        for p in OutputProfile::ALL {
            assert!(output_color_space(p).is_some());
        }
    }
}
