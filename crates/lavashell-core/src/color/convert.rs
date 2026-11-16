// SPDX-License-Identifier: GPL-3.0-only

use super::{Chromaticity, Gamut, GamutPrimaries};
use palette::convert::{FromColorUnclamped, IntoColorUnclamped};
use palette::encoding::{DisplayP3, FromLinear, IntoLinear, Rec2020, RecOetf, Srgb};
use palette::rgb::{LinDisplayP3, LinRec2020, LinSrgb, Primaries, RgbSpace};
use palette::white_point::{Any, D65, WhitePoint};
use palette::{Oklab, Xyz, Yxy};

pub(super) fn decode(channel: f64, gamut: Gamut) -> f64 {
    let magnitude = channel.abs();
    let linear = match gamut {
        Gamut::Rec2020 => <RecOetf as IntoLinear<f64, f64>>::into_linear(magnitude),
        Gamut::Srgb | Gamut::DisplayP3 => <Srgb as IntoLinear<f64, f64>>::into_linear(magnitude),
    };
    linear.copysign(channel)
}

pub(super) fn encode(channel: f64, gamut: Gamut) -> f64 {
    let magnitude = channel.abs();
    let encoded = match gamut {
        Gamut::Rec2020 => <RecOetf as FromLinear<f64, f64>>::from_linear(magnitude),
        Gamut::Srgb | Gamut::DisplayP3 => <Srgb as FromLinear<f64, f64>>::from_linear(magnitude),
    };
    encoded.copysign(channel)
}

pub(super) fn convert(channels: [f64; 3], from: Gamut, to: Gamut) -> [f64; 3] {
    if from == to {
        return channels;
    }
    let [red, green, blue] = channels;
    let xyz: Xyz<D65, f64> = match from {
        Gamut::Srgb => LinSrgb::new(red, green, blue).into_color_unclamped(),
        Gamut::DisplayP3 => LinDisplayP3::new(red, green, blue).into_color_unclamped(),
        Gamut::Rec2020 => LinRec2020::new(red, green, blue).into_color_unclamped(),
    };
    let (red, green, blue) = match to {
        Gamut::Srgb => LinSrgb::from_color_unclamped(xyz).into_components(),
        Gamut::DisplayP3 => LinDisplayP3::from_color_unclamped(xyz).into_components(),
        Gamut::Rec2020 => LinRec2020::from_color_unclamped(xyz).into_components(),
    };
    [red, green, blue]
}

pub(super) fn rgb_to_lab(channels: [f64; 3]) -> [f64; 3] {
    let [red, green, blue] = channels;
    let Oklab { l, a, b } = Oklab::from_color_unclamped(LinSrgb::new(red, green, blue));
    [l, a, b]
}

pub(super) fn lab_to_rgb(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let (red, green, blue) =
        LinSrgb::<f64>::from_color_unclamped(Oklab::new(l, a, b)).into_components();
    [red, green, blue]
}

pub(super) fn primaries(gamut: Gamut) -> GamutPrimaries {
    match gamut {
        Gamut::Srgb => primaries_of::<Srgb>(),
        Gamut::DisplayP3 => primaries_of::<DisplayP3>(),
        Gamut::Rec2020 => primaries_of::<Rec2020>(),
    }
}

fn primaries_of<S>() -> GamutPrimaries
where
    S: RgbSpace,
    S::Primaries: Primaries<f64>,
    S::WhitePoint: WhitePoint<f64>,
{
    let chromaticity = |color: Yxy<Any, f64>| Chromaticity {
        x: color.x,
        y: color.y,
    };
    GamutPrimaries {
        red: chromaticity(S::Primaries::red()),
        green: chromaticity(S::Primaries::green()),
        blue: chromaticity(S::Primaries::blue()),
        white: chromaticity(Yxy::from_color_unclamped(S::WhitePoint::get_xyz())),
    }
}
