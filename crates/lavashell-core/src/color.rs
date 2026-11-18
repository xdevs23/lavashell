// SPDX-License-Identifier: GPL-3.0-only

use crate::output::finest_step;
use convert::{convert, decode, encode, lab_to_rgb, rgb_to_lab};
use lavashell_enum_values::enum_with_values;
use palette::Srgba;
use palette::rgb::channels::Argb;
use std::hash::{Hash, Hasher};

mod convert;
#[cfg(all(test, feature = "serde"))]
mod encoding_tests;
mod interpolation;
#[cfg(test)]
pub(crate) mod samples;

pub use interpolation::{ColorIn, ColorInterpolation, HuePath};

const CHANNEL_MAX: f64 = u8::MAX as f64;

const DEGREES_PER_TURN: f64 = 360.0;

enum_with_values! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum Gamut {
        #[default]
        Srgb,
        DisplayP3,
        Rec2020,
    }

    pub(crate) const ALL;
}

impl Gamut {
    #[must_use]
    pub fn primaries(self) -> GamutPrimaries {
        convert::primaries(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticity {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamutPrimaries {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum Model {
    Oklch,
    Oklab,
    Encoded(Gamut),
    Linear(Gamut),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    model: Model,
    channels: [f64; 3],
    alpha: f64,
}

impl Default for Color {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

impl Color {
    pub const TRANSPARENT: Self = Self::linear([0.0; 3], 0.0, Gamut::Srgb);
    pub const WHITE: Self = Self::oklch(1.0, 0.0, 0.0);

    #[must_use]
    pub const fn oklch(lightness: f64, chroma: f64, hue: f64) -> Self {
        Self {
            model: Model::Oklch,
            channels: [lightness, chroma, hue],
            alpha: 1.0,
        }
    }

    #[must_use]
    pub const fn oklab(lightness: f64, a: f64, b: f64) -> Self {
        Self {
            model: Model::Oklab,
            channels: [lightness, a, b],
            alpha: 1.0,
        }
    }

    #[must_use]
    pub const fn rgb(hex: u32) -> Self {
        Self::rgba(hex, 255)
    }

    #[must_use]
    pub const fn rgba(hex: u32, alpha: u8) -> Self {
        Self::encoded(
            [low_byte(hex >> 16), low_byte(hex >> 8), low_byte(hex)],
            alpha as f64 / CHANNEL_MAX,
            Gamut::Srgb,
        )
    }

    #[must_use]
    pub const fn encoded(channels: [f64; 3], alpha: f64, gamut: Gamut) -> Self {
        Self {
            model: Model::Encoded(gamut),
            channels,
            alpha,
        }
    }

    #[must_use]
    pub const fn linear(channels: [f64; 3], alpha: f64, gamut: Gamut) -> Self {
        Self {
            model: Model::Linear(gamut),
            channels,
            alpha,
        }
    }

    #[must_use]
    pub const fn with_alpha(mut self, alpha: f64) -> Self {
        self.alpha = alpha;
        self
    }

    #[must_use]
    pub fn alpha(self) -> f64 {
        finite(self.alpha).clamp(0.0, 1.0)
    }

    pub fn to_linear(self, gamut: Gamut) -> [f64; 3] {
        if let Model::Linear(source) = self.model
            && source == gamut
        {
            return self.channels.map(finite);
        }
        match self.model {
            Model::Oklab | Model::Oklch => convert(lab_to_rgb(self.to_oklab()), Gamut::Srgb, gamut),
            Model::Linear(source) => convert(self.channels.map(finite), source, gamut),
            Model::Encoded(source) => convert(
                self.channels.map(|v| decode(finite(v), source)),
                source,
                gamut,
            ),
        }
    }

    #[must_use]
    pub fn to_encoded(self, gamut: Gamut) -> [f64; 3] {
        self.to_linear(gamut).map(|v| encode(v, gamut))
    }

    pub fn to_oklab(self) -> [f64; 3] {
        match self.model {
            Model::Oklab => self.channels.map(finite),
            Model::Oklch => {
                let [l, c, h] = self.channels.map(finite);
                let angle = h.to_radians();
                [l, c.max(0.0) * angle.cos(), c.max(0.0) * angle.sin()]
            }
            _ => rgb_to_lab(self.to_linear(Gamut::Srgb)),
        }
    }

    #[must_use]
    pub fn to_oklch(self) -> [f64; 3] {
        let [l, a, b] = self.to_oklab();
        [
            l,
            a.hypot(b),
            b.atan2(a).to_degrees().rem_euclid(DEGREES_PER_TURN),
        ]
    }

    #[must_use]
    pub fn in_gamut(self, gamut: Gamut) -> bool {
        let unseen = finest_step() / 2.0;
        let range = -unseen..=1.0 + unseen;
        (self.to_encoded(gamut).into_iter()).all(|channel| range.contains(&channel))
    }

    #[must_use]
    pub fn mapped(self, gamut: Gamut) -> Self {
        if self.in_gamut(gamut) {
            return self;
        }
        let [lightness, chroma, hue] = self.to_oklch();
        let lightness = lightness.clamp(0.0, 1.0);
        let (mut inside, mut outside) = (0.0, chroma);
        loop {
            let midpoint = f64::midpoint(inside, outside);
            if midpoint <= inside || midpoint >= outside {
                break;
            }
            if Self::oklch(lightness, midpoint, hue).in_gamut(gamut) {
                inside = midpoint;
            } else {
                outside = midpoint;
            }
        }
        Self::oklch(lightness, inside, hue).with_alpha(self.alpha())
    }

    #[must_use]
    pub fn mix(self, other: Self, amount: f64) -> Self {
        self.mix_in(other, amount, ColorInterpolation::default())
    }

    #[must_use]
    pub fn to_argb8(self) -> u32 {
        let [red, green, blue] = self.mapped(Gamut::Srgb).to_encoded(Gamut::Srgb);
        Srgba::new(red, green, blue, self.alpha())
            .into_format::<u8, u8>()
            .into_u32::<Argb>()
    }

    pub fn hash_components<H: Hasher>(&self, state: &mut H) {
        self.model.hash(state);
        self.channels.map(f64::to_bits).hash(state);
        self.alpha.to_bits().hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TonalPalette {
    pub hue: f64,
    pub chroma: f64,
}

impl TonalPalette {
    #[must_use]
    pub const fn new(hue: f64, chroma: f64) -> Self {
        Self { hue, chroma }
    }

    #[must_use]
    pub const fn tone(self, lightness: f64) -> Color {
        Color::oklch(lightness, self.chroma, self.hue)
    }
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

const fn low_byte(bits: u32) -> f64 {
    (bits & 0xff) as u8 as f64 / CHANNEL_MAX
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::samples::assert_shown_alike;
    use crate::float_asserts::{assert_f64_eq, assert_f64s_eq};

    #[test]
    fn primary_reference_and_roundtrips() {
        let red = Color::rgb(0x00ff_0000);
        assert_shown_alike(Color::oklab(0.627_955_4, 0.224_863_1, 0.125_846_3), red);
        for gamut in Gamut::ALL {
            let c = Color::encoded([0.17, 0.53, 0.91], 0.7, gamut);
            let [lightness, a, b] = c.to_oklab();
            assert_shown_alike(Color::oklab(lightness, a, b).with_alpha(c.alpha()), c);
        }
    }

    #[test]
    fn gamut_mapping_preserves_hue_and_lightness() {
        let c = Color::oklch(0.7, 0.35, 145.0);
        assert!(!c.in_gamut(Gamut::Srgb));
        let m = c.mapped(Gamut::Srgb);
        assert!(m.in_gamut(Gamut::Srgb));
        let r = m.to_oklch();
        assert_f64_eq(r[0], 0.7);
        assert_f64_eq(r[2], 145.0);
        assert!(r[1] < 0.35);
    }

    #[test]
    fn colors_on_the_edge_of_a_gamut_stay_inside_it_through_oklch() {
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
        ];
        for gamut in Gamut::ALL {
            for channels in corners {
                let [lightness, chroma, hue] = Color::encoded(channels, 1.0, gamut).to_oklch();
                let through_oklch = Color::oklch(lightness, chroma, hue);
                assert!(through_oklch.in_gamut(gamut), "{gamut:?} {channels:?}");
            }
        }
    }

    #[test]
    fn each_gamut_has_its_published_primaries_and_d65_white_to_its_four_published_decimals() {
        let rec2020 = Gamut::Rec2020.primaries();
        assert_eq!(rec2020.red, Chromaticity { x: 0.708, y: 0.292 });
        for gamut in Gamut::ALL {
            let white = gamut.primaries().white;
            let published_decimals = [(white.x * 1e4).round(), (white.y * 1e4).round()];
            assert_f64s_eq(&published_decimals, &[3127.0, 3290.0]);
        }
    }

    #[test]
    fn gamut_mapping_ends_at_the_largest_chroma_that_stays_inside() {
        let mapped = Color::oklch(0.7, 0.35, 145.0).mapped(Gamut::Srgb);
        let [lightness, chroma, hue] = mapped.channels;
        assert!(mapped.in_gamut(Gamut::Srgb));
        assert!(!Color::oklch(lightness, chroma.next_up(), hue).in_gamut(Gamut::Srgb));
    }

    #[test]
    fn headroom_and_fractional_precision_survive_authoring() {
        let channels = [4.0, 0.123_456, 0.5];
        let c = Color::linear(channels, 1.0, Gamut::Rec2020);
        assert_eq!(
            c.to_linear(Gamut::Rec2020).map(f64::to_bits),
            channels.map(f64::to_bits)
        );
        assert_eq!(Color::rgb(0x0012_3456).to_argb8(), 0xff12_3456);
        assert_eq!(
            Color::TRANSPARENT
                .mix(Color::rgb(0x00ff_0000), 0.5)
                .to_argb8(),
            0x80ff_0000
        );
    }
}
