// SPDX-License-Identifier: GPL-3.0-only
use super::super::PixelGeometry;
use super::tests::{new_painter, pixels, target};
use super::*;
use crate::test_error::TestError;
use crate::test_surfaces::floats;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};

#[test]
fn foreground_is_managed_but_rgb_and_center_coverage_remain_data() -> Result<(), TestError> {
    let info = ImageInfo::new((4, 4), ColorType::RGBA8888, AlphaType::Opaque, None);
    let mut rgb = skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = rgb.canvas().clear(skia_safe::Color::from_rgb(32, 128, 223));
    let mut center =
        skia_safe::surfaces::raster(&info.with_alpha_type(AlphaType::Premul), None, None)
            .ok_or(TestError::RasterSurface)?;
    let _chained_canvas = center
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
            crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
            color.to_linear(Gamut::Rec2020),
        ),
    ] {
        let info = ImageInfo::new((4, 4), ColorType::RGBAF32, AlphaType::Premul, space);
        let mut target =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let _chained_canvas = target.canvas().clear(skia_safe::Color::TRANSPARENT);
        let skia_color = crate::paint::color4f(color)?;
        blend::MaskBlend::new()?.draw(target.canvas(), &masks, skia_color)?;
        let mut bytes = [0; 16];
        assert!(target.read_pixels(&info.with_dimensions((1, 1)), &mut bytes, 16, (1, 1)));
        let actual: [f32; 4] = floats(&bytes);
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
        let style = TextStyle::new("probe", 13.sp(), Color::WHITE).resolve_unscaled();
        DrawCommand::Text {
            text: "Headphones p".into(),
            rect: Rect::new(7.25.dp(), 3.5.dp(), 280.dp(), 142.dp()),
            style: TextStyle { color, ..style },
        }
    };
    let mut warmed = new_painter(PixelGeometry::Rgb)?;
    let (mut output, info) = target()?;
    let _chained_canvas = output.canvas().clear(skia_safe::Color::new(0xff17_3967));
    warmed.paint_as_leaf(output.canvas(), &[command(Color::WHITE)], Density::ONE)?;
    let cached = |painter: &mut crate::Painter| {
        let tiles = painter.resources.tiles.get_mut().images().count();
        (painter.shaper.paragraphs().len(), tiles)
    };
    let before = cached(&mut warmed);
    for color in [Color::rgb(0x00a9_dd75), Color::rgb(0x006a_3fd0)] {
        let _chained_canvas = output.canvas().clear(skia_safe::Color::new(0xff17_3967));
        warmed.paint_as_leaf(output.canvas(), &[command(color)], Density::ONE)?;
        assert_eq!(cached(&mut warmed), before);
        let (mut fresh, _) = target()?;
        let _chained_canvas = fresh.canvas().clear(skia_safe::Color::new(0xff17_3967));
        new_painter(PixelGeometry::Rgb)?.paint_as_leaf(
            fresh.canvas(),
            &[command(color)],
            Density::ONE,
        )?;
        assert_eq!(pixels(&mut output, &info), pixels(&mut fresh, &info));
    }
    Ok(())
}
