// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{
    EFFECT_STRIPES, EFFECT_SURFACE, Pixels, linear_float_pixel, linear_float_pixels,
};
use crate::{Fonts, Painter};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, Surface};

pub fn sample(surface: &mut Surface, x: i32, y: i32) -> [f32; 4] {
    linear_float_pixel(surface, x, y, AlphaType::Unpremul)
}

pub fn sample_in_middle_row(surface: &mut Surface, x: i32) -> [f32; 4] {
    sample(surface, x, EFFECT_SURFACE.middle_row())
}

pub fn sample_at_center(surface: &mut Surface) -> [f32; 4] {
    sample_in_middle_row(surface, EFFECT_SURFACE.middle_column())
}

pub fn linear_pixels(surface: &mut Surface) -> Pixels<[f32; 4]> {
    linear_float_pixels(surface, AlphaType::Unpremul)
}

pub fn draw(surface: &mut Surface, paint: impl FnOnce(&mut Drawing<'_>)) -> Result<(), TestError> {
    draw_with(
        &mut Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?,
        surface,
        paint,
    )
}

pub fn draw_with(
    painter: &mut Painter,
    surface: &mut Surface,
    paint: impl FnOnce(&mut Drawing<'_>),
) -> Result<(), TestError> {
    let mut commands = Vec::new();
    let context = DrawContext::new(Viewport::new(EFFECT_SURFACE.size(), Density::ONE));
    paint(&mut Drawing::new(
        &mut commands,
        EFFECT_SURFACE.bounds(),
        &context,
    ));
    painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)
}

pub fn stripes(d: &mut Drawing<'_>) {
    EFFECT_STRIPES.fill_stripes(d, [Color::WHITE, Color::rgb(0)]);
}

pub fn assert_neutral_glass(s: &mut Surface) -> Result<(), TestError> {
    let _canvas_returned_for_chaining = s.canvas().clear(skia_safe::Color::TRANSPARENT);
    draw(s, |d| {
        let Size { width, height } = EFFECT_SURFACE.size();
        d.fill(Rect::new(0.dp(), 0.dp(), 20.dp(), height), Color::WHITE);
        d.fill(
            Rect::new(20.dp(), 0.dp(), width - 20.dp(), height),
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
    let c = sample_in_middle_row(s, 17);
    for channel in c {
        assert!(
            (channel - 1.0).abs() < 0.02,
            "neutral edge gained a color fringe: {c:?}"
        );
    }
    Ok(())
}

pub fn assert_continuous_glass(s: &mut Surface) -> Result<(), TestError> {
    for radius in [0.0, 12.0, 20.0] {
        let Size { width, height } = EFFECT_SURFACE.size();
        for end in [Point::new(width, 0.dp()), Point::new(0.dp(), height)] {
            let _canvas_returned_for_chaining = s.canvas().clear(skia_safe::Color::TRANSPARENT);
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
                d.fill(EFFECT_SURFACE.bounds(), ramp);
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
            let pixels = linear_pixels(s);
            let red = |x, y| pixels.at(x, y)[0];
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
