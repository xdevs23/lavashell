// SPDX-License-Identifier: GPL-3.0-only
//! Conversions of color channels, computed by the `palette` crate: between encoded and linear channels, between
//! the linear channels of two gamuts, and between linear sRGB and `OKLab`.

use super::Gamut;
use palette::convert::{FromColorUnclamped, IntoColorUnclamped};
use palette::encoding::{FromLinear, IntoLinear, RecOetf, Srgb};
use palette::rgb::{LinDisplayP3, LinRec2020, LinSrgb};
use palette::white_point::D65;
use palette::{Oklab, Xyz};

/// Decodes `channel`, encoded with the transfer function of `gamut`, into a linear channel. The function is
/// mirrored around zero: a negative channel decodes to the negative of what its magnitude decodes to.
pub(super) fn decode(channel: f64, gamut: Gamut) -> f64 {
    let magnitude = channel.abs();
    let linear = match gamut {
        Gamut::Rec2020 => <RecOetf as IntoLinear<f64, f64>>::into_linear(magnitude),
        Gamut::Srgb | Gamut::DisplayP3 => <Srgb as IntoLinear<f64, f64>>::into_linear(magnitude),
    };
    linear.copysign(channel)
}

/// Encodes the linear `channel` with the transfer function of `gamut`, mirrored around zero like `decode`.
pub(super) fn encode(channel: f64, gamut: Gamut) -> f64 {
    let magnitude = channel.abs();
    let encoded = match gamut {
        Gamut::Rec2020 => <RecOetf as FromLinear<f64, f64>>::from_linear(magnitude),
        Gamut::Srgb | Gamut::DisplayP3 => <Srgb as FromLinear<f64, f64>>::from_linear(magnitude),
    };
    encoded.copysign(channel)
}

/// Converts linear channels in the gamut `from` into linear channels in the gamut `to` through CIE XYZ. A channel
/// outside the gamut `to` is returned without clipping.
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

/// Converts linear sRGB channels into `OKLab`.
pub(super) fn rgb_to_lab(channels: [f64; 3]) -> [f64; 3] {
    let [red, green, blue] = channels;
    let Oklab { l, a, b } = Oklab::from_color_unclamped(LinSrgb::new(red, green, blue));
    [l, a, b]
}

/// Converts `OKLab` into linear sRGB channels.
pub(super) fn lab_to_rgb(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let (red, green, blue) =
        LinSrgb::<f64>::from_color_unclamped(Oklab::new(l, a, b)).into_components();
    [red, green, blue]
}
