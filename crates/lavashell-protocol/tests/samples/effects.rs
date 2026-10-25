// SPDX-License-Identifier: GPL-3.0-only
use super::{Check, Samples, TestResult};
use lavashell_core::effects::{BlendMode, BorderPlacement};

fn blend_modes() -> Samples<BlendMode> {
    every_variant! {
        BlendMode::Clear => BlendMode::Clear, [0]; BlendMode::Source => BlendMode::Source, [1];
        BlendMode::Destination => BlendMode::Destination, [2];
        BlendMode::SourceOver => BlendMode::SourceOver, [3];
        BlendMode::DestinationOver => BlendMode::DestinationOver, [4];
        BlendMode::SourceIn => BlendMode::SourceIn, [5];
        BlendMode::DestinationIn => BlendMode::DestinationIn, [6];
        BlendMode::SourceOut => BlendMode::SourceOut, [7];
        BlendMode::DestinationOut => BlendMode::DestinationOut, [8];
        BlendMode::SourceAtop => BlendMode::SourceAtop, [9];
        BlendMode::DestinationAtop => BlendMode::DestinationAtop, [10];
        BlendMode::Xor => BlendMode::Xor, [11]; BlendMode::Plus => BlendMode::Plus, [12];
        BlendMode::Modulate => BlendMode::Modulate, [13];
        BlendMode::Screen => BlendMode::Screen, [14];
        BlendMode::Overlay => BlendMode::Overlay, [15];
        BlendMode::Darken => BlendMode::Darken, [16];
        BlendMode::Lighten => BlendMode::Lighten, [17];
        BlendMode::ColorDodge => BlendMode::ColorDodge, [18];
        BlendMode::ColorBurn => BlendMode::ColorBurn, [19];
        BlendMode::HardLight => BlendMode::HardLight, [20];
        BlendMode::SoftLight => BlendMode::SoftLight, [21];
        BlendMode::Difference => BlendMode::Difference, [22];
        BlendMode::Exclusion => BlendMode::Exclusion, [23];
        BlendMode::Multiply => BlendMode::Multiply, [24]; BlendMode::Hue => BlendMode::Hue, [25];
        BlendMode::Saturation => BlendMode::Saturation, [26];
        BlendMode::Color => BlendMode::Color, [27];
        BlendMode::Luminosity => BlendMode::Luminosity, [28];
    }
}

fn border_placements() -> Samples<BorderPlacement> {
    every_variant! {
        BorderPlacement::Inside => BorderPlacement::Inside, [0];
        BorderPlacement::Center => BorderPlacement::Center, [1];
        BorderPlacement::Outside => BorderPlacement::Outside, [2];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&blend_modes())?;
    check.samples(&border_placements())
}
