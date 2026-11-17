// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_font::probe_style_at;
use crate::test_surfaces::{Pixels, encoded_pixels};
use crate::text::TextRendering;
use crate::text::sample_fixtures::{LABEL_SURFACE, mixed_script_label_text};
use lavashell_core::scene::tree::Packet;
use skia_safe::AlphaType;

fn text(size: f64, color: Color, rect: lavashell_core::geometry::Rect) -> DrawCommand {
    let mut style = probe_style_at(Sp::new(size));
    style.color = color;
    DrawCommand::Text {
        text: mixed_script_label_text(),
        rect,
        style,
    }
}

fn colored_edges(pixels: &Pixels<[u8; 4]>) -> usize {
    pixels
        .iter()
        .filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) >= 8)
        .count()
}

#[test]
fn opaque_rgb_coverage_survives_native_tiled_paint_and_safe_fallbacks() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut gpu = crate::VulkanRenderer::headless()?;
    let rect = lavashell_core::geometry::Rect::new(7.25.dp(), 4.5.dp(), 280.dp(), 36.dp());
    let mut painter = gpu
        .painter(&fonts)?
        .with_raster_mode(crate::raster::RasterMode::Tiles);
    painter.set_blend_space(crate::BlendSpace::Srgb);
    let results = native_results(&mut gpu, &mut painter, rect)?;
    assert_eq!(results[0].1, 0, "default grayscale acquired colored edges");
    assert!(
        results[1].1 > 20 && results[2].1 > 20,
        "missing native RGB/BGR coverage"
    );
    assert_ne!(
        results[1].2, results[2].2,
        "pixel geometry did not reverse coverage"
    );
    for result in &results[3..] {
        assert_eq!(
            result.1, 0,
            "unsafe native scope used RGB coverage: {}",
            result.0
        );
    }
    assert_native_color_formula(&mut gpu, &mut painter, rect, &results[1].2)?;
    assert_eq!(painter.render_stats().cache_bytes, 0);
    painter.retire_gpu_resources();
    Ok(())
}

fn policy(antialiasing: TextAntialiasing) -> TextRendering {
    TextRendering {
        hinting: super::super::Hinting::None,
        antialiasing,
        ..Default::default()
    }
}

type NativeResult = (&'static str, usize, Pixels<[u8; 4]>);

fn native_results(
    gpu: &mut crate::VulkanRenderer,
    painter: &mut crate::Painter,
    rect: lavashell_core::geometry::Rect,
) -> Result<Vec<NativeResult>, TestError> {
    let mut results = Vec::new();
    for (name, antialiasing, geometry, commands) in [
        (
            "gray",
            TextAntialiasing::Grayscale,
            super::super::PixelGeometry::Rgb,
            vec![text(13.0, Color::WHITE, rect)],
        ),
        (
            "rgb",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Rgb,
            vec![text(13.0, Color::WHITE, rect)],
        ),
        (
            "bgr",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Bgr,
            vec![text(13.0, Color::WHITE, rect)],
        ),
        (
            "unknown",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::None,
            vec![text(13.0, Color::WHITE, rect)],
        ),
        (
            "translucent",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Rgb,
            vec![text(13.0, Color::WHITE, rect)],
        ),
        (
            "fractional",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Rgb,
            vec![
                DrawCommand::Save,
                DrawCommand::Transform(Affine::translate(Point::new(0.25.dp(), 0.dp()))),
                text(13.0, Color::WHITE, rect),
                DrawCommand::Restore,
            ],
        ),
        (
            "rotated",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Rgb,
            vec![
                DrawCommand::Save,
                DrawCommand::Transform(Affine {
                    xx: 0.995,
                    xy: -0.1,
                    yx: 0.1,
                    yy: 0.995,
                    translation: Point::new(5.dp(), 0.dp()),
                }),
                text(13.0, Color::WHITE, rect),
                DrawCommand::Restore,
            ],
        ),
        (
            "isolated",
            TextAntialiasing::Subpixel,
            super::super::PixelGeometry::Rgb,
            vec![
                DrawCommand::Opacity(0.5),
                text(13.0, Color::WHITE, rect),
                DrawCommand::Restore,
            ],
        ),
    ] {
        let request = if name == "gray" {
            TextRendering::default()
        } else {
            policy(antialiasing)
        };
        let target_properties = super::super::TextDestination {
            opaque: name != "translucent",
            geometry,
        };
        painter.set_text_rendering(request, target_properties);
        let mut target = gpu.offscreen(LABEL_SURFACE.pixel_size())?;
        let _canvas_returned_for_chaining = target.canvas().clear(if name == "translucent" {
            skia_safe::Color::TRANSPARENT
        } else {
            skia_safe::Color::BLACK
        });
        painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut target)?;
        let pixels = encoded_pixels(&png, AlphaType::Unpremul)?;
        assert!(pixels.iter().any(|p| p[0] > 0), "empty native {name}");
        results.push((name, colored_edges(&pixels), pixels));
    }
    Ok(results)
}

fn assert_native_color_formula(
    gpu: &mut crate::VulkanRenderer,
    painter: &mut crate::Painter,
    rect: lavashell_core::geometry::Rect,
    mask: &Pixels<[u8; 4]>,
) -> Result<(), TestError> {
    painter.set_text_rendering(
        policy(TextAntialiasing::Subpixel),
        crate::test_surfaces::lcd_destination(),
    );
    let foreground = Color::rgb(0x00a9_dd75).with_alpha(0.4);
    let backdrop = Color::rgb(0x0017_3967);
    let background = DrawCommand::Fill {
        shape: LABEL_SURFACE.bounds().into(),
        brush: backdrop.into(),
    };
    let mut target = gpu.offscreen(LABEL_SURFACE.pixel_size())?;
    let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
    painter.paint_as_leaf(
        target.canvas(),
        &[background.clone(), text(13.0, foreground, rect)],
        Density::ONE,
    )?;
    let png = gpu.png(&mut target)?;
    let encoded = foreground.to_encoded(Gamut::Srgb);
    let backdrop = backdrop.to_encoded(Gamut::Srgb);
    let actual = encoded_pixels(&png, AlphaType::Unpremul)?;
    let mut max_error = 0.0_f64;
    for (coverage, actual) in mask.iter().zip(actual.iter()) {
        for channel in 0..3 {
            let amount = f64::from(coverage[channel]) / 255.0 * 0.4;
            let expected =
                ((encoded[channel] * amount + backdrop[channel] * (1.0 - amount)) * 255.0).round();
            max_error = max_error.max((f64::from(actual[channel]) - expected).abs());
        }
        assert_eq!(actual[3], 255);
    }
    assert!(
        max_error <= 4.0,
        "native foreground/color-space formula error: {max_error}"
    );
    let tree = crate::test_scenes::leaves([
        Packet::new(vec![background].into())?,
        Packet::new(vec![text(13.0, foreground, rect)].into())?,
    ])?;
    let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
    crate::test_scenes::paint(painter, target.canvas(), &tree, Density::ONE)?;
    let retained = encoded_pixels(&gpu.png(&mut target)?, AlphaType::Unpremul)?;
    let retained_error = actual.largest_channel_difference(&retained)?;
    assert!(
        retained_error <= 1,
        "retained native text baked a stale background"
    );
    Ok(())
}
