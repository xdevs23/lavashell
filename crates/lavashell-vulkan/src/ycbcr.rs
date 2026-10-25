// SPDX-License-Identifier: GPL-3.0-only
//! Encodes 8-bit R'G'B' colors into 8-bit YCbCr codes that the YCbCr sampler conversion of Vulkan decodes back to the
//! same color. The codes follow the range expansion and the model conversion in the Vulkan specification.

use crate::Error;
use ash::vk;
use num_traits::ToPrimitive;

/// The most that rounding to 8 bits moves a value, in 8-bit steps.
const ROUNDING: f64 = 0.5;

/// The standard whose weights of red, green and blue convert between YCbCr and RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Bt601,
    Bt709,
    Bt2020,
}

impl Model {
    /// The weights of red and blue in the luma, as ITU-R BT.601, BT.709 and BT.2020 define them, listed in sections
    /// 15.1.1 to 15.1.3 of the Khronos Data Format Specification 1.4:
    /// <https://registry.khronos.org/DataFormat/specs/1.4/dataformat.1.4.html#MODEL_BT709>
    fn weights(self) -> (f64, f64) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }
}

impl TryFrom<vk::SamplerYcbcrModelConversion> for Model {
    type Error = Error;

    fn try_from(model: vk::SamplerYcbcrModelConversion) -> Result<Self, Error> {
        match model {
            vk::SamplerYcbcrModelConversion::YCBCR_601 => Ok(Self::Bt601),
            vk::SamplerYcbcrModelConversion::YCBCR_709 => Ok(Self::Bt709),
            vk::SamplerYcbcrModelConversion::YCBCR_2020 => Ok(Self::Bt2020),
            model => Err(Error::YcbcrModel(model)),
        }
    }
}

/// Whether the codes of a component span all 8 bits, or leave room below and above as ITU-R BT.601 defines it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Full,
    Narrow,
}

impl Range {
    /// The 8-bit codes of luma and of chroma in this range, from the range expansion for 8 bits in the Vulkan
    /// specification:
    /// <https://docs.vulkan.org/spec/latest/chapters/textures.html#textures-sampler-YCbCr-conversion-rangeexpand>
    fn codes(self) -> [Codes; 2] {
        let full = f64::from(u8::MAX);
        let chroma_zero = f64::from(u8::MAX / 2 + 1);
        match self {
            Self::Full => [
                Codes {
                    zero: 0.0,
                    per_unit: full,
                },
                Codes {
                    zero: chroma_zero,
                    per_unit: full,
                },
            ],
            Self::Narrow => [
                Codes {
                    zero: 16.0,
                    per_unit: 219.0,
                },
                Codes {
                    zero: chroma_zero,
                    per_unit: 224.0,
                },
            ],
        }
    }
}

impl TryFrom<vk::SamplerYcbcrRange> for Range {
    type Error = Error;

    fn try_from(range: vk::SamplerYcbcrRange) -> Result<Self, Error> {
        match range {
            vk::SamplerYcbcrRange::ITU_FULL => Ok(Self::Full),
            vk::SamplerYcbcrRange::ITU_NARROW => Ok(Self::Narrow),
            range => Err(Error::YcbcrRange(range)),
        }
    }
}

/// The 8-bit codes of one component: the code of zero and the number of codes per unit.
#[derive(Clone, Copy)]
struct Codes {
    zero: f64,
    per_unit: f64,
}

impl Codes {
    fn encode(self, value: f64) -> Option<u8> {
        nearest_code(self.zero + value * self.per_unit)
    }
}

/// Encodes RGB colors into the YCbCr codes of one model and range.
pub struct Conversion {
    luma: Codes,
    chroma: Codes,
    red: f64,
    blue: f64,
}

impl Conversion {
    #[must_use]
    pub fn new(model: Model, range: Range) -> Self {
        let [luma, chroma] = range.codes();
        let (red, blue) = model.weights();
        Self {
            luma,
            chroma,
            red,
            blue,
        }
    }

    fn green(&self) -> f64 {
        1.0 - self.red - self.blue
    }

    /// The Y, Cb and Cr codes of `rgb`. The model conversion of Vulkan turns these codes back into `rgb` within
    /// `tolerance`. `None` when a code is not a number.
    #[must_use]
    pub fn encode(&self, rgb: [u8; 3]) -> Option<[u8; 3]> {
        let [red, green, blue] = rgb.map(|component| f64::from(component) / f64::from(u8::MAX));
        let luma = self.red * red + self.green() * green + self.blue * blue;
        let blue_difference = (blue - luma) / (2.0 * (1.0 - self.blue));
        let red_difference = (red - luma) / (2.0 * (1.0 - self.red));
        Some([
            self.luma.encode(luma)?,
            self.chroma.encode(blue_difference)?,
            self.chroma.encode(red_difference)?,
        ])
    }

    /// The matrix of the model conversion from YCbCr to RGB. The rows for red, green and blue each contain the weights
    /// of luma, of the blue difference and of the red difference.
    fn to_rgb(&self) -> [[f64; 3]; 3] {
        let green = self.green();
        [
            [1.0, 0.0, 2.0 * (1.0 - self.red)],
            [
                1.0,
                -2.0 * self.blue * (1.0 - self.blue) / green,
                -2.0 * self.red * (1.0 - self.red) / green,
            ],
            [1.0, 2.0 * (1.0 - self.blue), 0.0],
        ]
    }

    /// The most that a component drawn from the codes of `encode` may differ from the encoded RGB, in 8-bit steps.
    /// The codes are rounded once by `encode` and once more when Vulkan expands the range at the precision of the
    /// source. The decoded color is rounded once by the model conversion and once by the write into the pane. `None`
    /// when the tolerance exceeds 8 bits.
    pub fn tolerance(&self) -> Option<u8> {
        let steps_per_code =
            [self.luma, self.chroma, self.chroma].map(|codes| f64::from(u8::MAX) / codes.per_unit);
        let rounded_codes = (self.to_rgb().iter())
            .map(|row| {
                (row.iter().zip(steps_per_code))
                    .map(|(weight, steps)| weight.abs() * steps * ROUNDING)
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
        let bound = 2.0 * rounded_codes + 2.0 * ROUNDING;
        bound.ceil().to_u8()
    }
}

/// The 8-bit code nearest to `value`, clamped to the range from 0 to 255. `None` for a value that is not a number.
fn nearest_code(value: f64) -> Option<u8> {
    value.round().clamp(0.0, f64::from(u8::MAX)).to_u8()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("a YCbCr code of the color to convert is no number")]
        Codes,
        #[error("the tolerance of the YCbCr conversion exceeds 8 bits")]
        Tolerance,
    }

    #[test]
    fn white_and_black_take_the_ends_of_the_narrow_luma_range() {
        let narrow = Conversion::new(Model::Bt709, Range::Narrow);
        assert_eq!(narrow.encode([255, 255, 255]), Some([235, 128, 128]));
        assert_eq!(narrow.encode([0, 0, 0]), Some([16, 128, 128]));
    }

    #[test]
    fn decoding_the_codes_returns_the_color_within_the_tolerance() -> Result<(), TestError> {
        let conversions = [Model::Bt601, Model::Bt709, Model::Bt2020]
            .into_iter()
            .flat_map(|model| [Range::Full, Range::Narrow].map(|range| (model, range)));
        for (model, range) in conversions {
            let conversion = Conversion::new(model, range);
            let [luma, chroma] = range.codes();
            let tolerance = f64::from(conversion.tolerance().ok_or(TestError::Tolerance)?);
            for rgb in [[255, 0, 0], [0, 255, 0], [12, 34, 56]] {
                let [y, cb, cr] = conversion
                    .encode(rgb)
                    .ok_or(TestError::Codes)?
                    .map(f64::from);
                let differences = [
                    (y - luma.zero) / luma.per_unit,
                    (cb - chroma.zero) / chroma.per_unit,
                    (cr - chroma.zero) / chroma.per_unit,
                ];
                for (row, expected) in conversion.to_rgb().into_iter().zip(rgb) {
                    let component: f64 = row
                        .iter()
                        .zip(differences)
                        .map(|(weight, value)| weight * value)
                        .sum();
                    let drawn = (component.clamp(0.0, 1.0) * 255.0).round();
                    assert!((drawn - f64::from(expected)).abs() <= tolerance);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_vulkan_value_without_a_model_or_range_is_an_error() {
        let identity = Model::try_from(vk::SamplerYcbcrModelConversion::YCBCR_IDENTITY);
        assert!(matches!(identity, Err(Error::YcbcrModel(_))));
        let unknown = Range::try_from(vk::SamplerYcbcrRange::from_raw(2));
        assert!(matches!(unknown, Err(Error::YcbcrRange(_))));
    }

    #[test]
    fn a_value_outside_the_codes_takes_the_nearest_end() {
        assert_eq!(nearest_code(300.0), Some(255));
        assert_eq!(nearest_code(-4.0), Some(0));
        assert_eq!(nearest_code(127.5), Some(128));
        assert_eq!(nearest_code(f64::NAN), None);
    }
}
