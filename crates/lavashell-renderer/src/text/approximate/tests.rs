// SPDX-License-Identifier: GPL-3.0-only
use super::super::{PixelGeometry, TextDestination};
use super::*;
use crate::test_error::TestError;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo, Surface};

pub(super) fn new_painter(geometry: PixelGeometry) -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter =
        crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?.with_raster_paths(true);
    painter.set_text_rendering(
        TextRendering {
            antialiasing: TextAntialiasing::SubpixelApproximate,
            ..Default::default()
        },
        TextDestination {
            opaque: false,
            geometry,
        },
    );
    Ok(painter)
}

fn text(value: &str, size: f64, color: Color) -> DrawCommand {
    let mut style = TextStyle::new("probe", Sp::new(size), Color::WHITE).resolve_unscaled();
    style.color = color;
    DrawCommand::Text {
        text: value.into(),
        rect: Rect::new(7.25.dp(), 3.5.dp(), 280.dp(), 142.dp()),
        style,
    }
}

pub(super) fn target() -> Result<(Surface, ImageInfo), TestError> {
    let info = ImageInfo::new(
        (300, 150),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    Ok((
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?,
        info,
    ))
}

pub(super) fn pixels(surface: &mut Surface, info: &ImageInfo) -> Vec<u8> {
    let mut bytes = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(info, &mut bytes, info.min_row_bytes(), (0, 0)));
    bytes
}

#[test]
fn transparent_blend_raises_only_the_alpha_needed_for_valid_rgb() -> Result<(), TestError> {
    let info = ImageInfo::new((4, 4), ColorType::RGBA8888, AlphaType::Premul, None);
    let mut target =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = target.canvas().clear(skia_safe::Color::TRANSPARENT);
    let mut rgb = skia_safe::surfaces::raster(&info.with_alpha_type(AlphaType::Opaque), None, None)
        .ok_or(TestError::RasterSurface)?;
    let _chained_canvas = rgb
        .canvas()
        .clear(skia_safe::Color4f::new(0.125, 0.5, 0.875, 1.0));
    let mut center =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = center
        .canvas()
        .clear(skia_safe::Color4f::new(1.0, 1.0, 1.0, 0.5));
    let masks = masks::Masks {
        rgb: rgb.image_snapshot(),
        center: center.image_snapshot(),
        destination: skia_safe::Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
    };
    let white = crate::paint::color4f(Color::WHITE)?;
    blend::MaskBlend::new()?.draw(target.canvas(), &masks, white)?;
    let output = pixels(&mut target, &info);
    let pixel = &output[..4];
    assert!(
        pixel[0].abs_diff(32) <= 1 && pixel[1].abs_diff(128) <= 1 && pixel[2].abs_diff(223) <= 1
    );
    assert_eq!(pixel[3], pixel[2]);
    assert!(pixel[..3].iter().all(|channel| *channel <= pixel[3]));
    // The output pixel has one alpha for all three channels. A wallpaper composited under the pixel later is
    // therefore dimmed equally in every channel, although the mask has a different coverage for each channel. The
    // assertions below expect the resulting error in the red channel and none in the blue channel.
    let wallpaper = [0.8, 0.25, 0.5];
    let coverage = [0.125, 0.5, 0.875];
    let actual = std::array::from_fn::<_, 3, _>(|i| {
        f32::from(pixel[i]) / 255.0 + wallpaper[i] * (1.0 - f32::from(pixel[3]) / 255.0)
    });
    let ideal =
        std::array::from_fn::<_, 3, _>(|i| coverage[i] + wallpaper[i] * (1.0 - coverage[i]));
    assert!((actual[0] - ideal[0]).abs() > 0.59);
    assert!((actual[2] - ideal[2]).abs() < 0.01);
    Ok(())
}

#[test]
fn actual_small_and_clock_text_preserve_alpha_and_output_geometry() -> Result<(), TestError> {
    for size in [11.0, 13.0, 16.0, 100.0] {
        for alpha in [1.0, 0.4] {
            let value = if size < 20.0 { "Headphones p" } else { "12:34" };
            let command = text(value, size, Color::WHITE.with_alpha(alpha));
            let mut previous = None;
            let mut warmed = new_painter(PixelGeometry::Rgb)?;
            for geometry in [PixelGeometry::Rgb, PixelGeometry::Bgr, PixelGeometry::Rgb] {
                warmed.set_text_rendering(
                    warmed.text_rendering(),
                    TextDestination {
                        opaque: false,
                        geometry,
                    },
                );
                let (mut surface, info) = target()?;
                let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
                warmed.paint_as_leaf(
                    surface.canvas(),
                    std::slice::from_ref(&command),
                    Density::ONE,
                )?;
                let actual = pixels(&mut surface, &info);
                if size < 20.0 {
                    assert!(
                        actual
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .any(|p| p[0] != p[1] || p[1] != p[2])
                    );
                } else {
                    let (mut gray, _) = target()?;
                    let _chained_canvas = gray.canvas().clear(skia_safe::Color::TRANSPARENT);
                    new_painter(PixelGeometry::None)?.paint_as_leaf(
                        gray.canvas(),
                        std::slice::from_ref(&command),
                        Density::ONE,
                    )?;
                    // The mask blend and the native Skia drawing round differently by at most one step of the
                    // 8-bit target.
                    assert!(
                        actual
                            .iter()
                            .zip(pixels(&mut gray, &info))
                            .all(|(drawn, native)| drawn.abs_diff(native) <= 1),
                        "large raster text must keep native gray coverage"
                    );
                }
                assert!(
                    actual
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[..3].iter().all(|v| *v <= p[3]))
                );
                assert!(
                    actual
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| p[3] < 255 && p[3] > 0)
                );
                let (mut fresh, _) = target()?;
                let _chained_canvas = fresh.canvas().clear(skia_safe::Color::TRANSPARENT);
                new_painter(geometry)?.paint_as_leaf(
                    fresh.canvas(),
                    std::slice::from_ref(&command),
                    Density::ONE,
                )?;
                assert_eq!(actual, pixels(&mut fresh, &info));
                if let Some(previous) = previous
                    && size < 20.0
                {
                    assert_ne!(actual, previous);
                }
                previous = Some(actual);
            }
        }
    }
    Ok(())
}

#[test]
fn foreground_alpha_is_applied_before_overlapping_glyph_coverage_is_combined()
-> Result<(), TestError> {
    let mut painter = new_painter(PixelGeometry::Rgb)?;
    let mut command = text("HHH", 16.0, Color::WHITE.with_alpha(0.4));
    if let DrawCommand::Text { style, .. } = &mut command {
        style.letter_spacing = (-0.625).em();
    }
    let (mut target, info) = target()?;
    let _chained_canvas = target.canvas().clear(skia_safe::Color::TRANSPARENT);
    painter.paint_as_leaf(target.canvas(), &[command], Density::ONE)?;
    let output = pixels(&mut target, &info);
    assert!(
        output.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 125),
        "missing accumulated ink"
    );
    assert!(
        output
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[..3].iter().all(|v| *v <= p[3]))
    );
    Ok(())
}

#[test]
fn retained_and_transient_masks_follow_the_current_backdrop() -> Result<(), TestError> {
    let mut painter = new_painter(PixelGeometry::Rgb)?;
    let command = text(
        "Headphones p",
        13.0,
        Color::rgb(0x00a9_dd75).with_alpha(0.4),
    );
    let retained = crate::test_scenes::leaves([lavashell_core::scene::tree::Packet::new(
        vec![command.clone()].into(),
    )])?;
    for background in [0xf50d_111a, 0xff17_3967, 0x0000_0000, 0xf50d_111a] {
        let (mut warmed, info) = target()?;
        let _chained_canvas = warmed.canvas().clear(skia_safe::Color::new(background));
        crate::test_scenes::paint(&mut painter, warmed.canvas(), &retained, Density::ONE);
        let actual = pixels(&mut warmed, &info);
        let (mut fresh, _) = target()?;
        let _chained_canvas = fresh.canvas().clear(skia_safe::Color::new(background));
        new_painter(PixelGeometry::Rgb)?.paint_as_leaf(
            fresh.canvas(),
            std::slice::from_ref(&command),
            Density::ONE,
        )?;
        assert_eq!(actual, pixels(&mut fresh, &info));
        let (mut transient, _) = target()?;
        let _chained_canvas = transient.canvas().clear(skia_safe::Color::new(background));
        // Draws the masks once more directly through the rasterizer, inside `RasterCache::without_storage`, which
        // neither reuses nor stores pixels. Masks drawn this way must give the same pixels as the cached masks.
        let configuration = painter.shaper.configuration();
        let DrawCommand::Text { style, .. } = &command else {
            return Err(TestError::Missing("text command"));
        };
        let input = RasterInput {
            canvas: transient.canvas(),
            policy: configuration.rendering,
            destination: configuration.destination,
            opaque_scope: true,
            style,
            foreground: crate::paint::color4f(style.color)?,
        };
        let provider = ApproximateCoverage::new()?;
        let source_style = provider
            .source_style(&input)
            .ok_or(TestError::Missing("source style"))?;
        let source_input = painter.source_paragraph_input(
            "Headphones p",
            &source_style,
            280.dp(),
            provider.source_rendering(&input),
        )?;
        let cached = painter.shaper.cached(source_input);
        // Centers the paragraph vertically by its shaped height, as `Painter::paint_text` does.
        let origin = (
            7.25,
            3.5 + ((142.0 - cached.shaped.paragraph.height()) / 2.0).max(0.0),
        );
        let key = cached.key;
        let drawn = painter.resources.tiles.get_mut().without_storage(|tiles| {
            provider.draw(&input, origin, &mut cached.shaped, key, tiles)
        })?;
        assert_eq!(drawn, Drawn::Painted);
        assert_eq!(actual, pixels(&mut transient, &info));
    }
    assert_eq!(painter.render_stats().cache_bytes, 0);
    Ok(())
}

#[test]
fn unsafe_transforms_layers_and_oversized_masks_use_the_native_gray_path() -> Result<(), TestError>
{
    let command = text("Headphones p", 13.0, Color::WHITE);
    let mut warmed = new_painter(PixelGeometry::Rgb)?;
    let (mut surface, _) = target()?;
    warmed.paint_as_leaf(
        surface.canvas(),
        std::slice::from_ref(&command),
        Density::ONE,
    )?;
    let cases = [
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(Affine::translate(Point::new(0.25.dp(), 0.dp()))),
            command.clone(),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::Opacity(0.5),
            command.clone(),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(Affine {
                linear: [0.995, -0.1, 0.1, 0.995],
                translation: Point::new(5.dp(), 0.dp()),
            }),
            command.clone(),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::EffectLayer {
                bounds: Rect::new(0.dp(), 0.dp(), 300.dp(), 150.dp()),
                blend: lavashell_core::effects::BlendMode::SourceOver,
                blur: NonNegativeDp::ZERO,
                filter: None,
            },
            command,
            DrawCommand::Restore,
        ],
        vec![text("12:34", 100.0, Color::WHITE)],
        vec![text("Headphones p", 2000.0, Color::WHITE)],
    ];
    for commands in cases {
        let (mut approximate, info) = target()?;
        let (mut gray, _) = target()?;
        let _chained_canvas = approximate.canvas().clear(skia_safe::Color::TRANSPARENT);
        let _chained_canvas = gray.canvas().clear(skia_safe::Color::TRANSPARENT);
        warmed.paint_as_leaf(approximate.canvas(), &commands, Density::ONE)?;
        let mut painter = new_painter(PixelGeometry::None)?;
        painter.paint_as_leaf(gray.canvas(), &commands, Density::ONE)?;
        assert_eq!(pixels(&mut approximate, &info), pixels(&mut gray, &info));
    }
    Ok(())
}
