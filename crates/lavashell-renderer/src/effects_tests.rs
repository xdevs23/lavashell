// SPDX-License-Identifier: GPL-3.0-only
use crate::float_asserts::assert_f32_eq;
use crate::test_error::TestError;
use crate::test_surfaces::{ChannelRounding, floats};
use crate::{Error, Fonts, Painter};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

mod tangled;

pub(crate) use tangled::tangle;

pub(crate) fn surface() -> Result<Surface, TestError> {
    crate::test_surfaces::surface(96, 48)
}

pub(crate) fn sample(surface: &mut Surface, x: i32, y: i32) -> [f32; 4] {
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBAF32,
        AlphaType::Unpremul,
        skia_safe::ColorSpace::new_srgb_linear(),
    );
    let mut bytes = [0u8; 16];
    assert!(surface.read_pixels(&info, &mut bytes, 16, (x, y)));
    floats(&bytes)
}

pub(crate) fn draw(
    surface: &mut Surface,
    paint: impl FnOnce(&mut Drawing<'_>),
) -> Result<(), TestError> {
    draw_with(
        Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?,
        surface,
        paint,
    )
}

pub(crate) fn draw_with(
    mut painter: Painter,
    surface: &mut Surface,
    paint: impl FnOnce(&mut Drawing<'_>),
) -> Result<(), TestError> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(Size::new(96.dp(), 48.dp()), Density::ONE));
    paint(&mut Drawing::new(
        &mut commands,
        Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
        &context,
    ));
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)
}

pub(crate) fn stripes(d: &mut Drawing<'_>) {
    for i in 0..24_u8 {
        let color = if i % 2 == 0 {
            Color::WHITE
        } else {
            Color::rgb(0)
        };
        d.fill(
            Rect::new(4.dp() * f64::from(i), 0.dp(), 4.dp(), 48.dp()),
            color,
        );
    }
}

#[test]
fn floating_point_paint_preserves_hdr_headroom_and_fractional_channels() -> Result<(), TestError> {
    let mut s = surface()?;
    draw(&mut s, |d| {
        d.fill(
            Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
            Color::linear([2.0, 0.123_456, 0.5], 1.0, Gamut::Srgb),
        );
    })?;
    let c = sample(&mut s, 40, 24);
    for (channel, expected) in c.into_iter().zip([2.0, 0.123_456, 0.5, 1.0]) {
        let rounding = ChannelRounding::of(&s)?.after(1, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{c:?}");
    }
    Ok(())
}

#[test]
fn backdrop_blurs_content_in_linear_light_and_leaves_foreground_sharp() -> Result<(), TestError> {
    let mut s = surface()?;
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
    let blur = sample(&mut s, 35, 24);
    assert!((blur[0] - 0.5).abs() < 0.025, "{blur:?}");
    let foreground = sample(&mut s, 41, 21);
    assert!(foreground[0] > 0.98 && foreground[1] < 0.01 && foreground[2] < 0.01);
    assert!(sample(&mut s, 2, 24)[0] > 0.98);
    assert!(sample(&mut s, 6, 24)[0] < 0.01);
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
        let mut s = surface()?;
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
    let change = (sample(&mut plain, 20, 24)[0] - sample(&mut glass, 20, 24)[0]).abs();
    assert!(change > 0.25, "displacement changed {change}");
    let outside = f64::from(sample(&mut plain, 17, 5)[0]);
    // Each of the two renders stores the stripe once, so the two samples can differ by two roundings.
    let rounding = ChannelRounding::of(&plain)?.after(2, outside);
    assert!(
        (outside - f64::from(sample(&mut glass, 17, 5)[0])).abs() <= rounding,
        "outside rounded corner changed"
    );
    Ok(())
}

#[test]
fn glass_dispersion_preserves_neutral_color_across_transparent_edges() -> Result<(), TestError> {
    let mut s = surface()?;
    assert_neutral_glass(&mut s)
}

fn assert_neutral_glass(s: &mut Surface) -> Result<(), TestError> {
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    draw(s, |d| {
        d.fill(Rect::new(0.dp(), 0.dp(), 20.dp(), 48.dp()), Color::WHITE);
        d.fill(
            Rect::new(20.dp(), 0.dp(), 76.dp(), 48.dp()),
            Color::WHITE.with_alpha(0.25),
        );
        d.backdrop(
            Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()),
            constant!(Backdrop::glass(
                Dp::ZERO,
                Glass {
                    refraction: NonNegativeDp::ZERO,
                    dispersion: constant!(NonNegativeDp::try_new(Dp::new(4.0))),
                    rim: NonNegative::ZERO,
                },
            )),
        );
    })?;
    let c = sample(s, 17, 24);
    for channel in c {
        assert!(
            (channel - 1.0).abs() < 0.02,
            "neutral edge gained a color fringe: {c:?}"
        );
    }
    Ok(())
}

#[test]
fn glass_refraction_is_continuous_across_corner_diagonals() -> Result<(), TestError> {
    let mut s = surface()?;
    assert_continuous_glass(&mut s)
}

fn assert_continuous_glass(s: &mut Surface) -> Result<(), TestError> {
    // The source is a linear ramp of brightness along one axis, so a jump in the sampled position shows as a jump in
    // brightness.
    for radius in [0.0, 12.0, 20.0] {
        for end in [Point::new(96.dp(), 0.dp()), Point::new(0.dp(), 48.dp())] {
            let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
            let ramp = Gradient::linear(
                Point::new(0.dp(), 0.dp()),
                end,
                &[
                    Color::linear([0.05; 3], 1.0, Gamut::Srgb),
                    Color::linear([0.95; 3], 1.0, Gamut::Srgb),
                ],
            )?
            .interpolation(ColorInterpolation::LinearRgb);
            draw(s, |d| {
                d.fill(Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()), ramp);
                d.backdrop(
                    Shape::rounded(
                        Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()),
                        Dp::new(radius),
                    ),
                    constant!(Backdrop::glass(
                        Dp::ZERO,
                        Glass {
                            refraction: constant!(NonNegativeDp::try_new(Dp::new(20.0))),
                            dispersion: constant!(NonNegativeDp::try_new(Dp::new(0.7))),
                            rim: constant!(NonNegative::try_new(0.14)),
                        },
                    )),
                );
            })?;
            let info = ImageInfo::new(
                (96, 48),
                ColorType::RGBAF32,
                AlphaType::Unpremul,
                skia_safe::ColorSpace::new_srgb_linear(),
            );
            let mut pixels = vec![0u8; 96 * 48 * 16];
            assert!(s.read_pixels(&info, &mut pixels, 96 * 16, (0, 0)));
            let red = |x: usize, y: usize| floats::<1>(&pixels[(y * 96 + x) * 16..])[0];
            for y in 18..31 {
                for x in 26..70 {
                    for (a, c) in [
                        (red(x - 1, y), red(x + 1, y)),
                        (red(x, y - 1), red(x, y + 1)),
                    ] {
                        let curvature = (a - 2.0 * red(x, y) + c).abs();
                        assert!(
                            curvature < 0.018,
                            "lens seam at ({x}, {y}), radius {radius}, ramp {end:?}: {curvature}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn group_opacity_and_blending_apply_once_to_the_subtree() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    draw(&mut s, |d| {
        d.commands.push(DrawCommand::Opacity(0.5));
        d.fill(Rect::new(10.dp(), 10.dp(), 40.dp(), 25.dp()), Color::WHITE);
        d.fill(Rect::new(30.dp(), 10.dp(), 40.dp(), 25.dp()), Color::WHITE);
        d.restore();
    })?;
    let alpha = f64::from(sample(&mut s, 35, 20)[3]);
    // The alpha is stored twice: in the layer of the opacity group and when that layer is composited.
    let rounding = ChannelRounding::of(&s)?.after(2, 0.5);
    assert!((alpha - 0.5).abs() <= rounding, "{alpha}");
    Ok(())
}

#[test]
fn default_gradient_interpolation_matches_color_mix() -> Result<(), TestError> {
    let chromatic = Color::oklch(0.6, 0.1, 260.0);
    for from in [
        Color::oklch(0.6, 0.1, 150.0),
        Color::WHITE,
        Color::oklch(0.6, 0.000_003, 30.0),
    ] {
        let mut s = surface()?;
        let gradient = Gradient::linear(
            Point::new(0.dp(), 0.dp()),
            Point::new(96.dp(), 0.dp()),
            &[from, chromatic],
        )?;
        draw(&mut s, |d| {
            d.fill(Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()), gradient);
        })?;
        let middle = sample(&mut s, 47, 24);
        let expected = from.mix(chromatic, 47.5 / 96.0).to_linear(Gamut::Srgb);
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
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    let gradient = Gradient::linear(
        Point::new(0.dp(), 0.dp()),
        Point::new(96.dp(), 0.dp()),
        &[
            Color::rgb(0x00ff_0000).with_alpha(0.0),
            Color::rgb(0x0000_00ff),
        ],
    )?;
    draw(&mut s, |d| {
        d.fill(Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()), gradient);
    })?;
    let c = sample(&mut s, 47, 24);
    let alpha = 47.5 / 96.0;
    for (channel, expected) in c.into_iter().zip([0.0, 0.0, 1.0, alpha]) {
        // Reading the color unpremultiplied divides it by the stored alpha, which adds a second rounding.
        let rounding = ChannelRounding::of(&s)?.after(2, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{c:?}");
    }
    Ok(())
}

#[test]
fn shadows_and_stroke_placement_respect_the_surface_bounds() -> Result<(), TestError> {
    let mut s = surface()?;
    let _chained_canvas = s.canvas().clear(skia_safe::Color::TRANSPARENT);
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
    assert!(sample(&mut s, 52, 24)[3] > 0.98);
    assert!(sample(&mut s, 45, 24)[3] < 0.01);
    assert!(sample(&mut s, 68, 24)[3] > 0.98);
    assert!(sample(&mut s, 72, 24)[3] < 0.01);
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
    assert!(sample(&mut s, 22, 24)[3] > 0.98);
    assert!(sample(&mut s, 32, 24)[3] < 0.01);
    Ok(())
}

/// The ST 2084 (PQ) code value, from 0 to 1, for an absolute luminance of `nits` candela per square metre.
fn pq_encode(nits: f64) -> f64 {
    let p = (nits.clamp(0.0, 10000.0) / 10000.0).powf(2610.0 / 16384.0);
    ((3424.0 / 4096.0 + (2413.0 / 128.0) * p) / (1.0 + (2392.0 / 128.0) * p)).powf(2523.0 / 32.0)
}

#[test]
fn pq_conversion_uses_absolute_luminance_at_white_and_highlights() -> Result<(), TestError> {
    let mut s = surface()?;
    draw(&mut s, |d| {
        d.fill(Rect::new(0.dp(), 0.dp(), 48.dp(), 48.dp()), Color::WHITE);
        d.fill(
            Rect::new(48.dp(), 0.dp(), 48.dp(), 48.dp()),
            Color::linear([5.0; 3], 1.0, Gamut::Rec2020),
        );
    })?;
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBAF32,
        AlphaType::Unpremul,
        crate::color::output_color_space(OutputProfile::Hdr10)
            .ok_or(Error::OutputColorSpace(OutputProfile::Hdr10))?,
    );
    for (x, nits) in [(24, 203.0), (72, 1015.0)] {
        let mut bytes = [0u8; 16];
        assert!(s.read_pixels(&info, &mut bytes, 16, (x, 24)));
        let code = f64::from(floats::<1>(&bytes)[0]);
        let expected = pq_encode(nits);
        assert!(
            (code - expected).abs() <= ChannelRounding::of(&s)?.after(1, expected),
            "{code} at {nits} nits"
        );
    }
    Ok(())
}

#[test]
fn vulkan_f16_effects_preserve_hdr_headroom_and_sharp_foreground() -> Result<(), TestError> {
    let mut painter = crate::VulkanRenderer::headless()?;
    let mut s = painter.offscreen((96, 48))?;
    draw(&mut s, |d| {
        d.fill(
            Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()),
            Color::linear([2.0, 0.125, 0.5], 1.0, Gamut::Srgb),
        );
        d.backdrop(
            Shape::rounded(Rect::new(16.dp(), 4.dp(), 64.dp(), 40.dp()), 8.dp()),
            constant!(Backdrop::blur(Dp::new(3.0))),
        );
        d.fill(
            Rect::new(40.dp(), 20.dp(), 4.dp(), 4.dp()),
            Color::rgb(0x00ff_0000),
        );
    })?;
    let hdr = sample(&mut s, 24, 24);
    for (channel, expected) in hdr.into_iter().zip([2.0, 0.125]) {
        // The color is stored three times: by the fill, in the blurred copy of the fill made for the backdrop and in
        // the composite of that copy.
        let rounding = ChannelRounding::of(&s)?.after(3, expected);
        assert!((f64::from(channel) - expected).abs() <= rounding, "{hdr:?}");
    }
    let fg = sample(&mut s, 41, 21);
    assert!(fg[0] > 0.98 && fg[1] < 0.01);
    assert_neutral_glass(&mut s)?;
    assert_continuous_glass(&mut s)
}

#[test]
fn gradient_paths_preserve_canvas_origin_and_alpha_ramp() -> Result<(), TestError> {
    let mut surface = surface()?;
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

#[test]
fn a_restore_that_closes_nothing_is_ignored_and_recorded() -> Result<(), TestError> {
    let mut drawn = surface()?;
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let commands = [
        DrawCommand::Restore,
        DrawCommand::Fill {
            shape: Rect::new(0.dp(), 0.dp(), 96.dp(), 48.dp()).into(),
            brush: Color::WHITE.into(),
        },
    ];
    painter.paint_as_leaf(drawn.canvas(), &commands, Density::ONE)?;
    let white = sample(&mut drawn, 48, 24);
    let rounding = ChannelRounding::of(&drawn)?.after(1, 1.0);
    assert!(
        white
            .iter()
            .all(|channel| (f64::from(*channel) - 1.0).abs() <= rounding),
        "{white:?}"
    );
    assert_eq!(
        painter
            .failures
            .count(crate::failure::DrawFailure::UnbalancedRestore),
        1
    );
    Ok(())
}
