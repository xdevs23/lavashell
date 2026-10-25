// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_core::scene::tree::Packet;
use skia_safe::{ColorSpace, Data, Image};

fn text(size: f64, color: Color, rect: lavashell_core::geometry::Rect) -> DrawCommand {
    let mut style = TextStyle::new("probe", Sp::new(size), Color::WHITE).resolve_unscaled();
    style.color = color;
    DrawCommand::Text {
        text: "Mon 5 Oct e\u{301} office אבג".into(),
        rect,
        style,
    }
}

fn read(png: &[u8]) -> Result<Vec<u8>, TestError> {
    let image = Image::from_encoded(Data::new_copy(png)).ok_or(Error::DecodeImage)?;
    let info = ImageInfo::new(
        image.dimensions(),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        ColorSpace::new_srgb(),
    );
    let mut pixels = vec![0_u8; info.compute_min_byte_size()];
    assert!(image.read_pixels(
        &info,
        &mut pixels,
        info.min_row_bytes(),
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    ));
    Ok(pixels)
}

fn colored_edges(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]) >= 8)
        .count()
}

#[test]
fn opaque_rgb_coverage_survives_native_tiled_paint_and_safe_fallbacks() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut gpu = crate::VulkanRenderer::headless()?;
    let rect = lavashell_core::geometry::Rect::new(7.25.dp(), 4.5.dp(), 280.dp(), 36.dp());
    let mut painter = gpu.painter(&fonts)?.with_raster_paths(true);
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

type NativeResult = (&'static str, usize, Vec<u8>);

/// Draws the text of each native case with Vulkan. Returns for each case its name, the number of pixels with colored
/// edges, and the pixels.
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
                    linear: [0.995, -0.1, 0.1, 0.995],
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
        let mut target = gpu.offscreen((300, 48))?;
        let _chained_canvas = target.canvas().clear(if name == "translucent" {
            skia_safe::Color::TRANSPARENT
        } else {
            skia_safe::Color::BLACK
        });
        painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut target)?;
        let pixels = read(&png)?;
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p[0] > 0),
            "empty native {name}"
        );
        results.push((name, colored_edges(&pixels), pixels));
    }
    Ok(results)
}

/// Asserts that translucent subpixel text over an opaque fill has the colors blended in encoded sRGB with the
/// coverage in `mask`, both when it is drawn directly and when it is drawn from a retained scene tree.
fn assert_native_color_formula(
    gpu: &mut crate::VulkanRenderer,
    painter: &mut crate::Painter,
    rect: lavashell_core::geometry::Rect,
    mask: &[u8],
) -> Result<(), TestError> {
    painter.set_text_rendering(
        policy(TextAntialiasing::Subpixel),
        super::super::TextDestination {
            opaque: true,
            geometry: super::super::PixelGeometry::Rgb,
        },
    );
    let foreground = Color::rgb(0x00a9_dd75).with_alpha(0.4);
    let backdrop = Color::rgb(0x0017_3967);
    let background = DrawCommand::Fill {
        shape: lavashell_core::geometry::Rect::new(0.dp(), 0.dp(), 300.dp(), 48.dp()).into(),
        brush: backdrop.into(),
    };
    let mut target = gpu.offscreen((300, 48))?;
    let _chained_canvas = target.canvas().clear(skia_safe::Color::BLACK);
    painter.paint_as_leaf(
        target.canvas(),
        &[background.clone(), text(13.0, foreground, rect)],
        Density::ONE,
    )?;
    let png = gpu.png(&mut target)?;
    let encoded = foreground.to_encoded(Gamut::Srgb);
    let backdrop = backdrop.to_encoded(Gamut::Srgb);
    let actual = read(&png)?;
    let mut max_error = 0.0_f64;
    for (coverage, actual) in mask
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0.iter())
    {
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
        Packet::new(vec![background].into()),
        Packet::new(vec![text(13.0, foreground, rect)].into()),
    ])?;
    let _chained_canvas = target.canvas().clear(skia_safe::Color::BLACK);
    crate::test_scenes::paint(painter, target.canvas(), &tree, Density::ONE);
    let retained = read(&gpu.png(&mut target)?)?;
    let retained_error = actual
        .iter()
        .zip(&retained)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .ok_or(TestError::Missing("pixels"))?;
    assert!(
        retained_error <= 1,
        "retained native text baked a stale background"
    );
    Ok(())
}
