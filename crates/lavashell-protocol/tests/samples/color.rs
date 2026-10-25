// SPDX-License-Identifier: GPL-3.0-only
use super::{Check, Samples, TestResult};
use lavashell_core::color::{ColorInterpolation, Gamut, HuePath};
use lavashell_core::geometry::{Color, Point};
use lavashell_core::gradient::{GradientKind, TileMode};
use lavashell_core::units::DpExt;

pub(super) fn color() -> Color {
    Color::oklch(0.62, 0.18, 250.0)
}

fn gamuts() -> Samples<Gamut> {
    every_variant! {
        Gamut::Srgb => Gamut::Srgb, [0]; Gamut::DisplayP3 => Gamut::DisplayP3, [1];
        Gamut::Rec2020 => Gamut::Rec2020, [2];
    }
}

fn color_interpolations() -> Samples<ColorInterpolation> {
    every_variant! {
        ColorInterpolation::Oklab => ColorInterpolation::Oklab, [0];
        ColorInterpolation::Oklch(_) => ColorInterpolation::Oklch(HuePath::Decreasing), [1, 3];
        ColorInterpolation::LinearRgb => ColorInterpolation::LinearRgb, [2];
        ColorInterpolation::Srgb => ColorInterpolation::Srgb, [3];
    }
}

fn hue_paths() -> Samples<HuePath> {
    every_variant! {
        HuePath::Shorter => HuePath::Shorter, [0]; HuePath::Longer => HuePath::Longer, [1];
        HuePath::Increasing => HuePath::Increasing, [2];
        HuePath::Decreasing => HuePath::Decreasing, [3];
    }
}

fn gradient_kinds() -> Samples<GradientKind> {
    every_variant! {
        GradientKind::Linear { .. } => GradientKind::Linear {
            from: Point::new(1.dp(), 2.dp()),
            to: Point::new(30.dp(), 40.dp()),
        }, [
            0, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 62, 64, 0, 0,
            0, 0, 0, 0, 68, 64,
        ];
        GradientKind::Radial { .. } => GradientKind::Radial {
            center: Point::new(5.dp(), 9.dp()),
            radius: 30.dp(),
        }, [
            1, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 34, 64, 0, 0, 0, 0, 0, 0, 62, 64,
        ];
        GradientKind::Sweep { .. } => GradientKind::Sweep {
            center: Point::new(6.dp(), 8.dp()),
            start: 0.5,
            end: 2.5,
        }, [
            2, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 224, 63, 0, 0,
            0, 0, 0, 0, 4, 64,
        ];
        GradientKind::Conical { .. } => GradientKind::Conical {
            start: Point::new(1.dp(), 3.dp()),
            start_radius: 2.dp(),
            end: Point::new(20.dp(), 24.dp()),
            end_radius: 18.dp(),
        }, [
            3, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0,
            0, 0, 0, 52, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 50, 64,
        ];
    }
}

fn tile_modes() -> Samples<TileMode> {
    every_variant! {
        TileMode::Clamp => TileMode::Clamp, [0]; TileMode::Repeat => TileMode::Repeat, [1];
        TileMode::Mirror => TileMode::Mirror, [2]; TileMode::Decal => TileMode::Decal, [3];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&gamuts())?;
    check.samples(&color_interpolations())?;
    check.samples(&hue_paths())?;
    check.samples(&gradient_kinds())?;
    check.samples(&tile_modes())
}
