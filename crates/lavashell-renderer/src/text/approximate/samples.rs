// SPDX-License-Identifier: GPL-3.0-only
use super::super::{PixelGeometry, TextDestination};
use super::tests::{new_painter, pixels};
use super::*;
use crate::test_error::TestError;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo};

pub(super) fn commands() -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    for (background, column) in [
        Color::TRANSPARENT,
        Color::rgba(0x0011_111a, 245),
        Color::rgb(0x0011_111a),
    ]
    .into_iter()
    .zip(0_u8..)
    {
        let x = 300.dp() * f64::from(column);
        commands.push(DrawCommand::Fill {
            shape: Rect::new(x, 0.dp(), 300.dp(), 340.dp()).into(),
            brush: background.into(),
        });
        for (size, row) in [11.0, 13.0, 16.0].into_iter().zip(0_u8..) {
            for (alpha, line) in [1.0, 0.4].into_iter().zip(0_u8..) {
                let mut style =
                    TextStyle::new("probe", Sp::new(size), Color::WHITE).resolve_unscaled();
                style.color = Color::WHITE.with_alpha(alpha);
                commands.push(DrawCommand::Text {
                    text: "Headphones p".into(),
                    rect: Rect::new(
                        x + 12.25.dp(),
                        8.5.dp() + 52.dp() * f64::from(row) + 24.dp() * f64::from(line),
                        276.dp(),
                        24.dp(),
                    ),
                    style,
                });
            }
        }
        let mut style = TextStyle::new("probe", 100.sp(), Color::WHITE).resolve_unscaled();
        style.color = Color::WHITE;
        commands.push(DrawCommand::Text {
            text: "12:34".into(),
            rect: Rect::new(x + 12.25.dp(), 176.5.dp(), 276.dp(), 140.dp()),
            style,
        });
    }
    commands
}

#[test]
fn translucent_coverage_samples_stay_premultiplied_and_differ_per_pixel_geometry()
-> Result<(), TestError> {
    let commands = commands();
    let mut painter = new_painter(PixelGeometry::None)?;
    let mut gray = None;
    for geometry in [PixelGeometry::None, PixelGeometry::Rgb, PixelGeometry::Bgr] {
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
        painter.set_blend_space(crate::BlendSpace::Srgb);
        let info = ImageInfo::new(
            (900, 340),
            ColorType::RGBA8888,
            AlphaType::Premul,
            ColorSpace::new_srgb(),
        );
        let mut target =
            skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
        let _chained_canvas = target.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(target.canvas(), &commands, Density::ONE)?;
        let bytes = pixels(&mut target, &info);
        assert!(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[..3].iter().all(|v| *v <= p[3]))
        );
        if let Some(gray) = &gray {
            assert_ne!(gray, &bytes);
        } else {
            gray = Some(bytes);
        }
    }
    Ok(())
}
