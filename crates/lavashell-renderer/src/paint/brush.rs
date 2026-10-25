// SPDX-License-Identifier: GPL-3.0-only
//! The Skia paints of colors, brushes and gradients.

use crate::failure::DrawFailure;
use crate::paint::{sklength, skpoint};
use crate::scalar::{scalar, scalars};
use lavashell_core::prelude::*;
use skia_safe::Paint;

pub(crate) fn solid(color: Color) -> Result<Paint, DrawFailure> {
    let mut paint = Paint::default();
    let _chained_paint = paint
        .set_anti_alias(true)
        .set_dither(true)
        .set_color4f(color4f(color)?, &skia_safe::ColorSpace::new_srgb_linear());
    Ok(paint)
}

pub(super) fn brush_paint(brush: &Brush) -> Result<Paint, DrawFailure> {
    match brush {
        Brush::Solid(color) => solid(*color),
        Brush::Gradient(gradient) => gradient_paint(gradient),
    }
}

fn gradient_paint(gradient: &Gradient) -> Result<Paint, DrawFailure> {
    use skia_safe::gradient::{
        Colors, Gradient as SGradient, Interpolation, interpolation, shaders,
    };
    let mut paint = solid(Color::WHITE)?;
    let mut stops = gradient
        .stops
        .iter()
        .filter(|stop| stop.position.is_finite())
        .copied()
        .collect::<Vec<_>>();
    stops.sort_by(|left, right| left.position.total_cmp(&right.position));
    if stops.is_empty() {
        return solid(Color::TRANSPARENT);
    }
    if stops.len() == 1 {
        return solid(stops[0].color);
    }
    // Skia uses its own chroma threshold to tell whether a color has a hue. A stop without a hue gets zero chroma,
    // so Skia also interpolates it without a hue.
    let colors = stops
        .iter()
        .map(|stop| match gradient.interpolation {
            ColorInterpolation::Oklch(_) if !stop.color.has_hue() => {
                let [lightness, ..] = stop.color.to_oklch();
                color4f(Color::oklch(lightness, 0.0, 0.0).with_alpha(stop.color.alpha()))
            }
            _ => color4f(stop.color),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let positions = stops
        .iter()
        .map(|stop| scalar(stop.position.clamp(0.0, 1.0)))
        .collect::<Result<Vec<_>, _>>()?;
    let tile = match gradient.tile {
        TileMode::Clamp => skia_safe::TileMode::Clamp,
        TileMode::Repeat => skia_safe::TileMode::Repeat,
        TileMode::Mirror => skia_safe::TileMode::Mirror,
        TileMode::Decal => skia_safe::TileMode::Decal,
    };
    let space = match gradient.interpolation {
        ColorInterpolation::Oklab => interpolation::ColorSpace::OKLab,
        ColorInterpolation::Oklch(_) => interpolation::ColorSpace::OKLCH,
        ColorInterpolation::LinearRgb => interpolation::ColorSpace::SRGBLinear,
        ColorInterpolation::Srgb => interpolation::ColorSpace::SRGB,
    };
    let hue = match gradient.interpolation {
        ColorInterpolation::Oklch(HuePath::Longer) => interpolation::HueMethod::Longer,
        ColorInterpolation::Oklch(HuePath::Increasing) => interpolation::HueMethod::Increasing,
        ColorInterpolation::Oklch(HuePath::Decreasing) => interpolation::HueMethod::Decreasing,
        _ => interpolation::HueMethod::Shorter,
    };
    let skia_gradient = SGradient::new(
        Colors::new(
            &colors,
            Some(&positions),
            tile,
            skia_safe::ColorSpace::new_srgb_linear(),
        ),
        Interpolation {
            in_premul: interpolation::InPremul::Yes,
            color_space: space,
            hue_method: hue,
        },
    );
    let shader = match gradient.kind {
        GradientKind::Linear { from, to } => {
            shaders::linear_gradient((skpoint(from)?, skpoint(to)?), &skia_gradient, None)
        }
        GradientKind::Radial { center, radius } => {
            let circle = (skpoint(center)?, sklength(radius)?);
            shaders::radial_gradient(circle, &skia_gradient, None)
        }
        GradientKind::Sweep { center, start, end } => {
            let [start, end] = scalars([start, end])?;
            shaders::sweep_gradient(skpoint(center)?, (start, end), &skia_gradient, None)
        }
        GradientKind::Conical {
            start,
            start_radius,
            end,
            end_radius,
        } => shaders::two_point_conical_gradient(
            (skpoint(start)?, sklength(start_radius)?),
            (skpoint(end)?, sklength(end_radius)?),
            &skia_gradient,
            None,
        ),
    };
    if shader.is_none() {
        // The paint starts white. Without a shader for the geometry, the paint becomes transparent instead of white.
        let _chained_paint = paint.set_color(skia_safe::Color::TRANSPARENT);
    }
    let _chained_paint = paint.set_shader(shader);
    Ok(paint)
}

pub(crate) fn color4f(color: Color) -> Result<skia_safe::Color4f, DrawFailure> {
    let [r, g, b] = scalars(color.to_linear(Gamut::Srgb))?;
    Ok(skia_safe::Color4f::new(r, g, b, scalar(color.alpha())?))
}
