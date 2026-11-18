// SPDX-License-Identifier: GPL-3.0-only
use crate::effect_checks::{
    assert_continuous_glass, assert_neutral_glass, draw, sample, sample_at_center,
    sample_in_middle_row, stripes,
};
use crate::float_asserts::assert_f32_eq;
use crate::test_error::TestError;
use crate::test_surfaces::{ChannelRounding, EFFECT_SURFACE, float_pixel};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, Surface};

mod tangled;

pub(crate) use tangled::tangle;

#[test]
fn floating_point_paint_preserves_hdr_headroom_and_fractional_channels() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    draw(&mut s, |d| {
        d.fill(
            EFFECT_SURFACE.bounds(),
            Color::linear([2.0, 0.123_456, 0.5], 1.0, Gamut::Srgb),
        );
    })?;
    let c = sample_in_middle_row(&mut s, 40);
    for (channel, expected) in c.into_iter().zip([2.0, 0.123_456, 0.5, 1.0]) {
        let rounding = ChannelRounding::of(&s)?.after(1, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{c:?}");
    }
    Ok(())
}

#[test]
fn backdrop_blurs_content_in_linear_light_and_leaves_foreground_sharp() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    draw(&mut s, |d| {
        stripes(d);
        d.backdrop(
            Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()),
            constant!(Backdrop::blur(Dp::new(5.0))),
        );
        d.fill(
            Rect::new(40.dp(), 20.dp(), 4.dp(), 4.dp()),
            Color::rgb(0x00ff_0000),
        );
    })?;
    let blur = sample_in_middle_row(&mut s, 35);
    assert!((blur[0] - 0.5).abs() < 0.025, "{blur:?}");
    let foreground = sample(&mut s, 41, 21);
    assert!(foreground[0] > 0.98 && foreground[1] < 0.01 && foreground[2] < 0.01);
    assert!(sample_in_middle_row(&mut s, 2)[0] > 0.98);
    assert!(sample_in_middle_row(&mut s, 6)[0] < 0.01);
    Ok(())
}

#[test]
fn glass_displaces_only_its_backdrop_and_respects_rounded_bounds() -> Result<(), TestError> {
    let render = |glass| -> Result<Surface, TestError> {
        let effect = Backdrop::glass(
            Dp::ZERO,
            Glass {
                refraction: NonNegativeDp::try_new(glass)?,
                dispersion: NonNegativeDp::ZERO,
                rim: NonNegative::ZERO,
            },
        )?;
        let mut s = EFFECT_SURFACE.raster()?;
        draw(&mut s, |d| {
            stripes(d);
            d.backdrop(
                Shape::rounded(Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()), 12.dp()),
                effect,
            );
        })?;
        Ok(s)
    };
    let mut plain = render(0.dp())?;
    let mut glass = render(9.dp())?;
    let change =
        (sample_in_middle_row(&mut plain, 20)[0] - sample_in_middle_row(&mut glass, 20)[0]).abs();
    assert!(change > 0.25, "displacement changed {change}");
    let outside = f64::from(sample(&mut plain, 17, 5)[0]);
    let rounding = ChannelRounding::of(&plain)?.after(2, outside);
    assert!(
        (outside - f64::from(sample(&mut glass, 17, 5)[0])).abs() <= rounding,
        "outside rounded corner changed"
    );
    Ok(())
}

#[test]
fn glass_dispersion_preserves_neutral_color_across_transparent_edges() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    assert_neutral_glass(&mut s)
}

#[test]
fn glass_refraction_is_continuous_across_corner_diagonals() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    assert_continuous_glass(&mut s)
}

#[test]
fn group_opacity_and_blending_apply_once_to_the_subtree() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    let _canvas_returned_for_chaining = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    draw(&mut s, |d| {
        d.commands.push(DrawCommand::Opacity(0.5));
        d.fill(Rect::new(10.dp(), 10.dp(), 40.dp(), 25.dp()), Color::WHITE);
        d.fill(Rect::new(30.dp(), 10.dp(), 40.dp(), 25.dp()), Color::WHITE);
        d.restore();
    })?;
    let alpha = f64::from(sample(&mut s, 35, 20)[3]);
    let rounding = ChannelRounding::of(&s)?.after(2, 0.5);
    assert!((alpha - 0.5).abs() <= rounding, "{alpha}");
    Ok(())
}

fn fraction_of_width_at_center() -> f64 {
    (f64::from(EFFECT_SURFACE.middle_column()) + 0.5) / EFFECT_SURFACE.size().width.get()
}

#[test]
fn default_gradient_interpolation_matches_color_mix() -> Result<(), TestError> {
    let chromatic = Color::oklch(0.6, 0.1, 260.0);
    for from in [
        Color::oklch(0.6, 0.1, 150.0),
        Color::WHITE,
        Color::oklch(0.6, 0.000_003, 30.0),
    ] {
        let mut s = EFFECT_SURFACE.raster()?;
        let gradient = Gradient::linear(
            Point::new(0.dp(), 0.dp()),
            Point::new(EFFECT_SURFACE.size().width, 0.dp()),
            &[from, chromatic],
        )?;
        draw(&mut s, |d| {
            d.fill(EFFECT_SURFACE.bounds(), gradient);
        })?;
        let middle = sample_at_center(&mut s);
        let expected = (from.mix(chromatic, fraction_of_width_at_center())).to_linear(Gamut::Srgb);
        for (channel, expected) in middle.into_iter().zip(expected) {
            let rounding = ChannelRounding::of(&s)?.after(1, expected);
            assert!(
                (f64::from(channel) - expected).abs() <= rounding,
                "{from:?}: {middle:?} {expected}"
            );
        }
    }
    Ok(())
}

#[test]
fn transparent_gradient_endpoint_does_not_introduce_a_colored_fringe() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    let _canvas_returned_for_chaining = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let gradient = Gradient::linear(
        Point::new(0.dp(), 0.dp()),
        Point::new(EFFECT_SURFACE.size().width, 0.dp()),
        &[
            Color::rgb(0x00ff_0000).with_alpha(0.0),
            Color::rgb(0x0000_00ff),
        ],
    )?;
    draw(&mut s, |d| {
        d.fill(EFFECT_SURFACE.bounds(), gradient);
    })?;
    let c = sample_at_center(&mut s);
    let alpha = fraction_of_width_at_center();
    for (channel, expected) in c.into_iter().zip([0.0, 0.0, 1.0, alpha]) {
        let rounding = ChannelRounding::of(&s)?.after(2, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{c:?}");
    }
    Ok(())
}

#[test]
fn shadows_and_stroke_placement_respect_the_surface_bounds() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    let _canvas_returned_for_chaining = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    draw(&mut s, |d| {
        let rect = Rect::new(20.dp(), 10.dp(), 30.dp(), 28.dp());
        d.shadow(
            rect,
            constant!(Shadow::new(
                Color::WHITE,
                Dp::ZERO,
                Point::new(Dp::new(6.0), Dp::ZERO),
            )),
        );
        d.stroke(
            Rect::new(70.dp(), 10.dp(), 16.dp(), 28.dp()),
            constant!(Stroke::new(Color::WHITE, Dp::new(4.0))).placement(BorderPlacement::Outside),
        );
    })?;
    assert!(sample_in_middle_row(&mut s, 52)[3] > 0.98);
    assert!(sample_in_middle_row(&mut s, 45)[3] < 0.01);
    assert!(sample_in_middle_row(&mut s, 68)[3] > 0.98);
    assert!(sample_in_middle_row(&mut s, 72)[3] < 0.01);
    draw(&mut s, |d| {
        d.shadow(
            Rect::new(20.dp(), 10.dp(), 30.dp(), 28.dp()),
            constant!(Shadow::new(
                Color::WHITE,
                Dp::ZERO,
                Point::new(Dp::new(6.0), Dp::ZERO),
            ))
            .inset(true),
        );
    })?;
    assert!(sample_in_middle_row(&mut s, 22)[3] > 0.98);
    assert!(sample_in_middle_row(&mut s, 32)[3] < 0.01);
    Ok(())
}

fn pq_encode(nits: f64) -> f64 {
    let p = (nits.clamp(0.0, 10000.0) / 10000.0).powf(2610.0 / 16384.0);
    ((3424.0 / 4096.0 + (2413.0 / 128.0) * p) / (1.0 + (2392.0 / 128.0) * p)).powf(2523.0 / 32.0)
}

#[test]
fn pq_conversion_uses_absolute_luminance_at_white_and_highlights() -> Result<(), TestError> {
    let mut s = EFFECT_SURFACE.raster()?;
    draw(&mut s, |d| {
        let Size { width, height } = EFFECT_SURFACE.size();
        let half = width / 2.0;
        d.fill(Rect::new(0.dp(), 0.dp(), half, height), Color::WHITE);
        d.fill(
            Rect::new(half, 0.dp(), half, height),
            Color::linear([5.0; 3], 1.0, Gamut::Rec2020),
        );
    })?;
    let hdr10 = crate::color::output_color_space(OutputProfile::Hdr10)
        .ok_or(TestError::Missing("output color space"))?;
    let (left_middle, right_middle) = (
        EFFECT_SURFACE.middle_column() / 2,
        EFFECT_SURFACE.middle_column() * 3 / 2,
    );
    for (x, nits) in [(left_middle, 203.0), (right_middle, 1015.0)] {
        let position = (x, EFFECT_SURFACE.middle_row());
        let code =
            f64::from(float_pixel(&mut s, position, AlphaType::Unpremul, Some(hdr10.clone()))[0]);
        let expected = pq_encode(nits);
        assert!(
            (code - expected).abs() <= ChannelRounding::of(&s)?.after(1, expected),
            "{code} at {nits} nits"
        );
    }
    Ok(())
}

#[test]
fn gradient_paths_preserve_canvas_origin_and_alpha_ramp() -> Result<(), TestError> {
    let mut surface = EFFECT_SURFACE.raster()?;
    let gradient = Gradient::linear(
        Point::new(0.dp(), 0.dp()),
        Point::new(0.dp(), 40.dp()),
        &[Color::WHITE, Color::WHITE.with_alpha(0.0)],
    )?;
    draw(&mut surface, |drawing| {
        drawing.bounds = Rect::new(12.dp(), 4.dp(), 40.dp(), 40.dp());
        drawing.path(
            &[
                Point::new(0.dp(), 0.dp()),
                Point::new(40.dp(), 0.dp()),
                Point::new(40.dp(), 40.dp()),
                Point::new(0.dp(), 40.dp()),
            ],
            true,
            gradient,
            None,
        );
    })?;
    let top = sample(&mut surface, 30, 8);
    let bottom = sample(&mut surface, 30, 39);
    assert!(top[3] > 0.85 && bottom[3] < 0.15, "{top:?}, {bottom:?}");
    assert_f32_eq(sample(&mut surface, 8, 8)[3], 0.0);
    Ok(())
}
