// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f32_eq;
use crate::test_error::TestError;
use crate::test_surfaces::floats;
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, ImageInfo, Surface};

fn pixel(surface: &mut Surface, info: &ImageInfo) -> [f32; 4] {
    let mut data = [0_u8; 16];
    assert!(surface.read_pixels(info, &mut data, 16, (0, 0)));
    floats(&data)
}

#[test]
fn coverage_remains_data_and_foreground_uses_destination_color_management() -> Result<(), TestError>
{
    let blend = RgbBlend::new()?;
    let mask_info = ImageInfo::new(
        (1, 1),
        ColorType::RGBA8888,
        AlphaType::Opaque,
        ColorSpace::new_srgb(),
    );
    let mut mask =
        skia_safe::surfaces::raster(&mask_info, None, None).ok_or(TestError::RasterSurface)?;
    assert!(
        mask.canvas()
            .write_pixels(&mask_info, &[37, 127, 223, 255], 4, (0, 0))
    );
    let coverage = [37.0 / 255.0, 127.0 / 255.0, 223.0 / 255.0];
    for profile in [
        OutputProfile::Srgb,
        OutputProfile::ScRgb,
        OutputProfile::DisplayP3,
    ] {
        let info = ImageInfo::new(
            (1, 1),
            ColorType::RGBAF32,
            AlphaType::Premul,
            crate::color::output_color_space(profile).ok_or(Error::OutputColorSpace(profile))?,
        );
        for gamut in [Gamut::Srgb, Gamut::DisplayP3] {
            for alpha in [0.0_f32, 0.4, 1.0] {
                let color = Color::encoded([0.8, 0.2, 0.4], f64::from(alpha), gamut);
                let mut foreground = skia_safe::surfaces::raster(&info, None, None)
                    .ok_or(TestError::RasterSurface)?;
                let _chained_canvas = foreground.canvas().clear(skia_safe::Color::TRANSPARENT);
                let mut painter = crate::Painter::new(
                    &crate::Fonts::new(),
                    crate::UniformRange::VULKAN_REQUIRED,
                )?;
                painter.paint_as_leaf(
                    foreground.canvas(),
                    &[DrawCommand::Fill {
                        shape: lavashell_core::geometry::Rect::new(0.dp(), 0.dp(), 1.dp(), 1.dp())
                            .into(),
                        brush: color.into(),
                    }],
                    Density::ONE,
                )?;
                let foreground = pixel(&mut foreground, &info);
                let backdrop = [0.05_f32, 0.2, 0.4, 1.0];
                let bytes: Vec<_> = backdrop.iter().flat_map(|v| v.to_ne_bytes()).collect();
                let mut result = skia_safe::surfaces::raster(&info, None, None)
                    .ok_or(TestError::RasterSurface)?;
                assert!(result.canvas().write_pixels(&info, &bytes, 16, (0, 0)));
                blend.draw(
                    result.canvas(),
                    &mask.image_snapshot(),
                    skia_safe::Rect::from_wh(1.0, 1.0),
                    crate::paint::color4f(color)?,
                )?;
                let result = pixel(&mut result, &info);
                for channel in 0..3 {
                    let expected = coverage[channel] * foreground[channel]
                        + (1.0 - coverage[channel] * alpha) * backdrop[channel];
                    assert!(
                        (result[channel] - expected).abs() <= 0.002,
                        "{profile:?}/{gamut:?}/{alpha}: {result:?}, expected={expected}"
                    );
                }
                assert_f32_eq(result[3], 1.0);
            }
        }
    }
    Ok(())
}
