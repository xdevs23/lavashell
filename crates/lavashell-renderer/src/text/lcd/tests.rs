// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use crate::test_font::probe_style;
use crate::test_surfaces::{has_colored_fringe, pixels, srgb_info};
use crate::text::TextRendering;
use crate::text::sample_fixtures::{LABEL_SURFACE, mixed_script_label_text};
use skia_safe::Surface;

fn new_painter() -> Result<crate::Painter, TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut painter = crate::Painter::new(&fonts, crate::UniformRange::VULKAN_REQUIRED)?
        .with_raster_mode(crate::raster::RasterMode::Tiles);
    painter.set_text_rendering(
        TextRendering {
            hinting: super::super::Hinting::None,
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        },
        crate::test_surfaces::lcd_destination(),
    );
    Ok(painter)
}

fn text(color: Color) -> DrawCommand {
    let mut style = probe_style();
    style.color = color;
    style.spans.push(lavashell_core::scene::TextSpan {
        range: 0..3,
        weight: Some(lavashell_core::scene::FontWeight::BOLD),
        italic: true,
        family: None,
    });
    DrawCommand::Text {
        text: mixed_script_label_text(),
        rect: lavashell_core::geometry::Rect::new(7.25.dp(), 4.5.dp(), 280.dp(), 36.dp()),
        style,
    }
}

fn surface() -> Result<Surface, TestError> {
    skia_safe::surfaces::raster(&srgb_info(LABEL_SURFACE.dimensions()), None, None)
        .ok_or(TestError::RasterSurface)
}

#[test]
fn actual_coverage_reuses_masks_across_foreground_and_backdrop_changes() -> Result<(), TestError> {
    let mut painter = new_painter()?;
    let mut target = surface()?;
    let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
    painter.paint_as_leaf(target.canvas(), &[text(Color::WHITE)], Density::ONE)?;
    let mask = pixels(&mut target);
    assert!(has_colored_fringe(&mask));
    for alpha in [1.0, 0.4, 0.0, 1.0] {
        for background in [0x0017_3967, 0x0067_421c, 0x0017_3967] {
            let foreground = Color::rgb(0x00a9_dd75).with_alpha(alpha);
            let encoded = foreground.to_encoded(Gamut::Srgb);
            let backdrop = Color::rgb(background).to_encoded(Gamut::Srgb);
            let [_alpha_replaced_by_opaque, red, green, blue] = u32::to_be_bytes(background);
            let _canvas_returned_for_chaining = target
                .canvas()
                .clear(skia_safe::Color::from_argb(255, red, green, blue));
            painter.paint_as_leaf(target.canvas(), &[text(foreground)], Density::ONE)?;
            let actual = pixels(&mut target);
            for (coverage, result) in mask.iter().zip(actual.iter()) {
                for channel in 0..3 {
                    let amount = f64::from(coverage[channel]) / 255.0 * alpha;
                    let expected =
                        ((encoded[channel] * amount + backdrop[channel] * (1.0 - amount)) * 255.0)
                            .round();
                    assert!(
                        (f64::from(result[channel]) - expected).abs() <= 2.0,
                        "{result:?}/{expected}"
                    );
                }
                assert_eq!(result[3], 255);
            }
        }
    }
    Ok(())
}

#[test]
fn output_geometry_changes_do_not_reuse_the_other_channels_mask() -> Result<(), TestError> {
    let mut warmed = new_painter()?;
    let mut previous = None;
    for geometry in [
        super::super::PixelGeometry::Rgb,
        super::super::PixelGeometry::Bgr,
        super::super::PixelGeometry::Rgb,
    ] {
        let destination = super::super::TextDestination {
            opaque: true,
            geometry,
        };
        let policy = warmed.text_rendering();
        warmed.set_text_rendering(policy, destination);
        let mut target = surface()?;
        let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
        warmed.paint_as_leaf(target.canvas(), &[text(Color::WHITE)], Density::ONE)?;
        let actual = pixels(&mut target);
        let mut fresh = surface()?;
        let _canvas_returned_for_chaining = fresh.canvas().clear(skia_safe::Color::BLACK);
        let mut painter = new_painter()?;
        painter.set_text_rendering(policy, destination);
        painter.paint_as_leaf(fresh.canvas(), &[text(Color::WHITE)], Density::ONE)?;
        assert_eq!(actual, pixels(&mut fresh));
        if let Some(previous) = previous {
            assert_ne!(
                actual, previous,
                "RGB and BGR must have different edge coverage"
            );
        }
        previous = Some(actual);
    }
    Ok(())
}

#[test]
fn isolated_opacity_and_fractional_or_rotated_transforms_use_grayscale_tiles()
-> Result<(), TestError> {
    let mut painter = new_painter()?;
    let cases = [
        vec![
            DrawCommand::Opacity(0.5),
            text(Color::WHITE),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(Affine::translate(Point::new(0.25.dp(), 0.dp()))),
            text(Color::WHITE),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::Save,
            DrawCommand::Transform(Affine {
                xx: 0.995,
                xy: -0.1,
                yx: 0.1,
                yy: 0.995,
                translation: Point::new(5.dp(), 0.dp()),
            }),
            text(Color::WHITE),
            DrawCommand::Restore,
        ],
        vec![
            DrawCommand::EffectLayer {
                bounds: LABEL_SURFACE.bounds(),
                blend: lavashell_core::effects::BlendMode::SourceOver,
                blur: NonNegativeDp::ZERO,
                filter: None,
            },
            text(Color::WHITE),
            DrawCommand::Restore,
        ],
    ];
    for commands in cases {
        let mut target = surface()?;
        let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
        painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
        let actual = pixels(&mut target);
        assert!(actual.iter().any(|p| p[0] > 0), "empty glyph fixture");
        assert!(actual.iter().all(|p| p[0] == p[1] && p[1] == p[2]));
    }
    Ok(())
}

#[test]
fn destination_reading_retained_text_is_not_baked_into_a_transparent_layer() -> Result<(), TestError>
{
    use lavashell_core::scene::tree::Packet;
    let mut painter = new_painter()?;
    let tree = crate::test_scenes::leaves([Packet::new(vec![text(Color::WHITE)].into())?])?;
    for background in [
        skia_safe::Color::BLACK,
        skia_safe::Color::BLUE,
        skia_safe::Color::BLACK,
    ] {
        let mut retained = surface()?;
        let _canvas_returned_for_chaining = retained.canvas().clear(background);
        crate::test_scenes::paint(&mut painter, retained.canvas(), &tree, Density::ONE)?;
        let mut direct = surface()?;
        let _canvas_returned_for_chaining = direct.canvas().clear(background);
        new_painter()?.paint_as_leaf(direct.canvas(), &[text(Color::WHITE)], Density::ONE)?;
        assert_eq!(pixels(&mut retained), pixels(&mut direct));
    }
    assert_eq!(painter.render_stats().cache_bytes, 0);
    assert_eq!(painter.render_stats().uncached_leaves, 3);
    Ok(())
}

#[test]
fn a_text_color_change_reuses_the_paragraph_and_the_mask() -> Result<(), TestError> {
    let mut warmed = new_painter()?;
    let mut target = surface()?;
    let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
    warmed.paint_as_leaf(target.canvas(), &[text(Color::WHITE)], Density::ONE)?;
    let cached = |painter: &mut crate::Painter| {
        let tiles = painter.resources.tiles.get_mut().images().count();
        (painter.shaper.paragraphs().len(), tiles)
    };
    let before = cached(&mut warmed);
    for color in [Color::rgb(0x00a9_dd75), Color::rgb(0x006a_3fd0)] {
        let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::BLACK);
        warmed.paint_as_leaf(target.canvas(), &[text(color)], Density::ONE)?;
        assert_eq!(cached(&mut warmed), before);
        let mut fresh = surface()?;
        let _canvas_returned_for_chaining = fresh.canvas().clear(skia_safe::Color::BLACK);
        new_painter()?.paint_as_leaf(fresh.canvas(), &[text(color)], Density::ONE)?;
        assert_eq!(pixels(&mut target), pixels(&mut fresh));
    }
    Ok(())
}
