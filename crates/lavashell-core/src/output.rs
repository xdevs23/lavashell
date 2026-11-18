// SPDX-License-Identifier: GPL-3.0-only

use crate::geometry::Size;
use lavashell_enum_values::enum_with_values;

enum_with_values! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum OutputProfile {
        #[default]
        Srgb,
        DisplayP3,
        Rec2020,
        Hdr10,
        ScRgb,
    }

    pub const ALL;
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum Precision {
        #[default]
        Eight,
        Ten,
        Twelve,
        Float16,
    }

    pub const ALL;
}

impl Precision {
    const fn significand_bits(self) -> u8 {
        match self {
            Self::Eight => 8,
            Self::Ten => 10,
            Self::Twelve => 12,
            Self::Float16 => HALF_FLOAT_SIGNIFICAND_BITS,
        }
    }

    pub(crate) fn step(self) -> f64 {
        0.5_f64.powi(i32::from(self.significand_bits()))
    }
}

// https://doc.rust-lang.org/std/primitive.f16.html#associatedconstant.MANTISSA_DIGITS
const HALF_FLOAT_SIGNIFICAND_BITS: u8 = 11;

pub(crate) fn finest_step() -> f64 {
    Precision::ALL
        .into_iter()
        .map(Precision::step)
        .fold(f64::INFINITY, f64::min)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputRequest {
    pub profile: OutputProfile,
    pub precision: Precision,
}

impl OutputRequest {
    #[must_use]
    pub const fn new(profile: OutputProfile, precision: Precision) -> Self {
        Self { profile, precision }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    pub requested: OutputRequest,
    pub profile: OutputProfile,
    pub color_bits: u8,
    pub alpha_bits: u8,
    pub floating_point: bool,
    pub fallback: bool,
}

const PQ_PEAK_LUMINANCE: f64 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdrMetadata {
    pub mastering_gamut: crate::color::Gamut,
    pub max_luminance: f64,
    pub min_luminance: f64,
    pub max_content_light_level: f64,
    pub max_frame_average_light_level: f64,
}

impl HdrMetadata {
    #[must_use]
    pub fn valid(self) -> bool {
        [
            self.max_luminance,
            self.min_luminance,
            self.max_content_light_level,
            self.max_frame_average_light_level,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0 && *v <= PQ_PEAK_LUMINANCE)
            && self.max_luminance > self.min_luminance
            && self.max_frame_average_light_level <= self.max_content_light_level
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Millimetres(pub f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalSize {
    pub width: Millimetres,
    pub height: Millimetres,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: Option<String>,
    pub size: Size,
    pub physical_size: Option<PhysicalSize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finest_step_is_no_coarser_than_the_step_of_any_precision() {
        for precision in Precision::ALL {
            assert!(finest_step() <= precision.step());
        }
    }
}
