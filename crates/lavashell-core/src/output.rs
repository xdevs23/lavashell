// SPDX-License-Identifier: GPL-3.0-only
//! The color profile and the channel precision that an application requests for an output, and what the output
//! provides. A surface profile agreed with the platform, such as HDR10, does not mean that the display shows HDR.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputProfile {
    #[default]
    Srgb,
    DisplayP3,
    Rec2020,
    Hdr10,
    ScRgb,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Precision {
    #[default]
    Eight,
    Ten,
    Twelve,
    Float16,
}

impl Precision {
    pub const ALL: [Self; 4] = [Self::Eight, Self::Ten, Self::Twelve, Self::Float16];

    /// The number of bits that distinguish the values of a channel from 0 to 1: the depth of an integer channel, or
    /// the significand of a half float with its implicit bit, as IEEE 754 defines binary16.
    #[must_use]
    pub const fn bits(self) -> u8 {
        match self {
            Self::Eight => 8,
            Self::Ten => 10,
            Self::Twelve => 12,
            Self::Float16 => 11,
        }
    }
}

/// One step of a color channel at the highest bit depth of any output. No output shows a change of a channel that
/// is smaller than half of this step.
pub(crate) fn finest_step() -> f64 {
    let deepest =
        (Precision::ALL.iter()).fold(0, |deepest, precision| precision.bits().max(deepest));
    0.5_f64.powi(i32::from(deepest))
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

/// The brightest luminance in cd/m² that the PQ transfer function encodes, as SMPTE ST 2084 defines it.
const PQ_PEAK_LUMINANCE: f64 = 10_000.0;

/// HDR mastering metadata in cd/m², which an application may set on its output surface. The values describe the
/// content drawn by the application.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finest_step_is_no_coarser_than_the_step_of_any_precision() {
        for precision in Precision::ALL {
            assert!(finest_step() <= 0.5_f64.powi(i32::from(precision.bits())));
        }
    }
}
