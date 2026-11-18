// SPDX-License-Identifier: GPL-3.0-only
use super::super::PixelGeometry;
use super::samples::new_painter;
use super::tests::target;
use super::*;
use crate::test_error::TestError;
use crate::test_font::probe_style;
use crate::test_surfaces::{float_info, float_pixel, pixels};
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};

#[test]
fn foreground_is_managed_but_rgb_and_center_coverage_remain_data() -> Result<(), TestError> {
    let info = ImageInfo::new((4, 4), ColorType::RGBA8888, AlphaType::Opaque, None);
    let mut rgb = skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining =
        rgb.canvas().clear(skia_safe::Color::from_rgb(32, 128, 223));
    let mut center =
        skia_safe::surfaces::raster(&info.with_alpha_type(AlphaType::Premul), None, None)
            .ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = center
        .canvas()
        .clear(skia_safe::Color::from_argb(128, 255, 255, 255));
    let masks = masks::Masks {
        rgb: rgb.image_snapshot(),
        center: center.image_snapshot(),
        destination: skia_safe::Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
    };
    let color = Color::rgb(0x00a9_dd75);
    for (space, foreground) in [
        (ColorSpace::new_srgb(), color.to_encoded(Gamut::Srgb)),
        (ColorSpace::new_srgb_linear(), color.to_linear(Gamut::Srgb)),
        (
            crate::color::working_color_space()?,
            color.to_linear(Gamut::Rec2020),
        ),
    ] {
        let info = float_info((4, 4), AlphaType::Premul, Some(space));
        let mut target =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let _canvas_returned_for_chaining = target.canvas().clear(skia_safe::Color::TRANSPARENT);
        let skia_color = crate::paint::color4f(color)?;
        let blend = blend::MaskBlend::new()?;
        blend.draw(target.canvas(), &blend.coverage(&masks)?, skia_color)?;
        let actual = float_pixel(&mut target, (1, 1), AlphaType::Premul, info.color_space());
        let expected = [
            32.0 / 255.0 * foreground[0],
            128.0 / 255.0 * foreground[1],
            223.0 / 255.0 * foreground[2],
        ];
        for (actual, expected) in actual[..3].iter().zip(expected) {
            assert!(
                (f64::from(*actual) - expected).abs() <= 0.0003,
                "foreground/mask space changed: {actual}/{expected}"
            );
        }
        let alpha = expected.iter().copied().fold(128.0 / 255.0, f64::max);
        assert!((f64::from(actual[3]) - alpha).abs() <= 0.0003);
        assert!(
            actual[..3]
                .iter()
                .all(|channel| *channel <= actual[3] + 0.00001)
        );
    }
    Ok(())
}

#[test]
fn a_text_color_change_reuses_the_paragraph_and_the_masks() -> Result<(), TestError> {
    let command = |color: Color| {
        let style = probe_style();
        DrawCommand::Text {
            text: "Headphones p".into(),
            rect: Rect::new(7.25.dp(), 3.5.dp(), 280.dp(), 142.dp()),
            style: TextStyle { color, ..style },
        }
    };
    let mut warmed = new_painter(PixelGeometry::Rgb)?;
    let mut output = target()?;
    let _canvas_returned_for_chaining = output.canvas().clear(skia_safe::Color::new(0xff17_3967));
    warmed.paint_as_leaf(output.canvas(), &[command(Color::WHITE)], Density::ONE)?;
    let cached = |painter: &mut crate::Painter| {
        let tiles = painter.resources.tiles.get_mut().images().count();
        (painter.shaper.paragraphs().len(), tiles)
    };
    let before = cached(&mut warmed);
    for color in [Color::rgb(0x00a9_dd75), Color::rgb(0x006a_3fd0)] {
        let _canvas_returned_for_chaining =
            output.canvas().clear(skia_safe::Color::new(0xff17_3967));
        warmed.paint_as_leaf(output.canvas(), &[command(color)], Density::ONE)?;
        assert_eq!(cached(&mut warmed), before);
        let mut fresh = target()?;
        let _canvas_returned_for_chaining =
            fresh.canvas().clear(skia_safe::Color::new(0xff17_3967));
        new_painter(PixelGeometry::Rgb)?.paint_as_leaf(
            fresh.canvas(),
            &[command(color)],
            Density::ONE,
        )?;
        assert_eq!(pixels(&mut output), pixels(&mut fresh));
    }
    Ok(())
}
