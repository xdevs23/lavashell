// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::{Error, Fonts, Painter, decode_image, encoded_image, raster_png};
use lavashell_core::prelude::*;
use lavashell_test_platform::WithoutFailures;
use skia_safe::{AlphaType, ColorSpace, ColorType, EncodedImageFormat, ImageInfo};
use std::time::Duration;

fn encoded(format: EncodedImageFormat) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        (8, 8),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _chained_canvas = surface
        .canvas()
        .clear(skia_safe::Color::from_rgb(48, 96, 144));
    let data = surface
        .image_snapshot()
        .encode(None, format, 100)
        .ok_or(TestError::Missing("encoded image"))?;
    Ok(data.as_bytes().to_vec())
}

fn scene(bytes: Vec<u8>) -> Result<Vec<DrawCommand>, TestError> {
    let image = decode_image(bytes)?;
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let image = image.clone();
        ui.canvas(Size::new(8.dp(), 8.dp()), Modifier::new(), move |canvas| {
            canvas.image(
                &image,
                Rect::new(0.dp(), 0.dp(), 8.dp(), 8.dp()),
                ImageFit::Stretch,
            );
        });
    });
    let mut shaper = crate::text::Shaper::new(&Fonts::new(), crate::failure::Failures::default())?;
    let frame = view.frame(
        Viewport::new(Size::new(8.dp(), 8.dp()), Density::ONE),
        &mut shaper,
    );
    crate::test_scenes::leaf_commands(frame.without_failures()?)
}

fn pixel(bytes: &[u8]) -> Result<[u8; 4], TestError> {
    let image = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(bytes))
        .ok_or(Error::DecodeImage)?;
    let info = ImageInfo::new(
        (1, 1),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        ColorSpace::new_srgb(),
    );
    let mut pixel = [0u8; 4];
    assert!(image.read_pixels(
        &info,
        &mut pixel,
        4,
        (4, 4),
        skia_safe::image::CachingHint::Disallow
    ));
    Ok(pixel)
}

fn assert_color(bytes: &[u8], format: EncodedImageFormat) -> Result<(), TestError> {
    let pixel = pixel(bytes)?;
    for (actual, expected) in pixel.into_iter().zip([48u8, 96, 144, 255]) {
        assert!(actual.abs_diff(expected) <= 3, "{format:?}: {pixel:?}");
    }
    Ok(())
}

#[test]
fn jpeg_and_png_pixels_survive_the_canvas_and_raster_export_path() -> Result<(), TestError> {
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        let bytes = encoded(format)?;
        assert_color(&bytes, format)?;
        let tree = SceneTree::with_leaf(scene(bytes)?)?;
        let png = raster_png(tree.at(Duration::ZERO), (8, 8), &Fonts::new())?;
        assert_color(&png, format)?;
    }
    Ok(())
}

#[test]
fn jpeg_and_png_pixels_survive_vulkan_upload() -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    let mut painter = gpu.painter(&Fonts::new())?;
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        let commands = scene(encoded(format)?)?;
        let mut surface = gpu.offscreen((8, 8))?;
        let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        painter.paint_as_leaf(surface.canvas(), &commands, Density::ONE)?;
        let png = gpu.png(&mut surface)?;
        assert_color(&png, format)?;
    }
    Ok(())
}

#[test]
fn decoded_cpu_asset_is_shared_with_the_painter_without_a_second_decode() -> Result<(), TestError> {
    let image = decode_image(encoded(EncodedImageFormat::JPEG)?)?;
    let decoded = image
        .backend_data::<skia_safe::Image>()
        .ok_or(TestError::Missing("Skia image in the decoded image"))?;
    assert!(!decoded.is_texture_backed());
    let identity = decoded.unique_id();
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let mut surface =
        skia_safe::surfaces::raster_n32_premul((8, 8)).ok_or(TestError::RasterSurface)?;
    painter.paint_as_leaf(
        surface.canvas(),
        &[DrawCommand::Image {
            image: image.clone(),
            destination: Rect::new(0.dp(), 0.dp(), 8.dp(), 8.dp()),
            frame: Rect::new(0.dp(), 0.dp(), 8.dp(), 8.dp()),
        }],
        Density::ONE,
    )?;
    assert_eq!(painter.images[&image.id()].1.unique_id(), identity);
    Ok(())
}

#[test]
fn an_encoded_image_takes_the_size_that_its_header_states() -> Result<(), TestError> {
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        assert_eq!(encoded_image(encoded(format)?)?.dimensions(), (8, 8));
    }
    Ok(())
}
