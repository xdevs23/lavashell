// SPDX-License-Identifier: GPL-3.0-only

use ash::vk;
use lavashell_enum_values::enum_with_values;
use num_traits::ToPrimitive;

const ROUNDING: f64 = 0.5;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the model {0:?} does not convert between YCbCr and RGB")]
    Model(vk::SamplerYcbcrModelConversion),
    #[error("the YCbCr range {0:?} is unknown")]
    Range(vk::SamplerYcbcrRange),
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Model {
        Bt601,
        Bt709,
        Bt2020,
    }

    pub const ALL;
}

impl Model {
    // https://registry.khronos.org/DataFormat/specs/1.4/dataformat.1.4.html#MODEL_BT709
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
            model => Err(Error::Model(model)),
        }
    }
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Range {
        Full,
        Narrow,
    }

    pub const ALL;
}

impl Range {
    // https://docs.vulkan.org/spec/latest/chapters/textures.html#textures-sampler-YCbCr-conversion-rangeexpand
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
            range => Err(Error::Range(range)),
        }
    }
}

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

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];
    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    #[test]
    fn colors_encode_to_fixed_codes_for_each_model_and_range() {
        let expected = [
            (
                Model::Bt601,
                Range::Full,
                [
                    (RED, [76, 85, 255]),
                    (GREEN, [150, 44, 21]),
                    (BLUE, [29, 255, 107]),
                    (WHITE, [255, 128, 128]),
                    (BLACK, [0, 128, 128]),
                ],
            ),
            (
                Model::Bt601,
                Range::Narrow,
                [
                    (RED, [81, 90, 240]),
                    (GREEN, [145, 54, 34]),
                    (BLUE, [41, 240, 110]),
                    (WHITE, [235, 128, 128]),
                    (BLACK, [16, 128, 128]),
                ],
            ),
            (
                Model::Bt709,
                Range::Full,
                [
                    (RED, [54, 99, 255]),
                    (GREEN, [182, 30, 12]),
                    (BLUE, [18, 255, 116]),
                    (WHITE, [255, 128, 128]),
                    (BLACK, [0, 128, 128]),
                ],
            ),
            (
                Model::Bt709,
                Range::Narrow,
                [
                    (RED, [63, 102, 240]),
                    (GREEN, [173, 42, 26]),
                    (BLUE, [32, 240, 118]),
                    (WHITE, [235, 128, 128]),
                    (BLACK, [16, 128, 128]),
                ],
            ),
            (
                Model::Bt2020,
                Range::Full,
                [
                    (RED, [67, 92, 255]),
                    (GREEN, [173, 36, 11]),
                    (BLUE, [15, 255, 118]),
                    (WHITE, [255, 128, 128]),
                    (BLACK, [0, 128, 128]),
                ],
            ),
            (
                Model::Bt2020,
                Range::Narrow,
                [
                    (RED, [74, 97, 240]),
                    (GREEN, [164, 47, 25]),
                    (BLUE, [29, 240, 119]),
                    (WHITE, [235, 128, 128]),
                    (BLACK, [16, 128, 128]),
                ],
            ),
        ];
        for (model, range, colors) in expected {
            let conversion = Conversion::new(model, range);
            for (rgb, codes) in colors {
                assert_eq!(
                    conversion.encode(rgb),
                    Some(codes),
                    "{model:?} {range:?} {rgb:?}"
                );
            }
        }
    }

    #[test]
    fn decoding_the_codes_returns_the_color_within_the_tolerance() -> Result<(), TestError> {
        let conversions =
            (Model::ALL.into_iter()).flat_map(|model| Range::ALL.map(|range| (model, range)));
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
        assert!(matches!(identity, Err(Error::Model(_))));
        let unknown = Range::try_from(vk::SamplerYcbcrRange::from_raw(2));
        assert!(matches!(unknown, Err(Error::Range(_))));
    }

    #[test]
    fn a_value_outside_the_codes_takes_the_nearest_end() {
        assert_eq!(nearest_code(300.0), Some(255));
        assert_eq!(nearest_code(-4.0), Some(0));
        assert_eq!(nearest_code(127.5), Some(128));
        assert_eq!(nearest_code(f64::NAN), None);
    }
}
