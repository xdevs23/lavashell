// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{TestSurface, encoded_pixels};
use crate::{Fonts, Painter, decode_image, encoded_image, raster_png};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, EncodedImageFormat, ImageInfo};
use std::time::Duration;

const IMAGE: TestSurface = TestSurface::new(8, 8);

fn encoded(format: EncodedImageFormat) -> Result<Vec<u8>, TestError> {
    let info = ImageInfo::new(
        IMAGE.dimensions(),
        ColorType::RGBA8888,
        AlphaType::Premul,
        ColorSpace::new_srgb(),
    );
    let mut surface =
        skia_safe::surfaces::raster(&info, None, None).ok_or(TestError::RasterSurface)?;
    let _canvas_returned_for_chaining = surface
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
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let image = image.clone();
        ui.canvas(IMAGE.size(), Modifier::new(), move |canvas| {
            canvas.image(&image, IMAGE.bounds(), ImageFit::Stretch);
        });
    });
    let mut painter = Painter::new(&Fonts::new(), crate::UniformRange::VULKAN_REQUIRED)?;
    let viewport = Viewport::new(IMAGE.size(), Density::ONE);
    crate::test_scenes::framed_commands(&mut view, viewport, &mut painter)
}

fn assert_color(bytes: &[u8], format: EncodedImageFormat) -> Result<(), TestError> {
    let pixel = encoded_pixels(bytes, AlphaType::Unpremul)?.at(4, 4);
    for (actual, expected) in pixel.into_iter().zip([48u8, 96, 144, 255]) {
        assert!(actual.abs_diff(expected) <= 3, "{format:?}: {pixel:?}");
    }
    Ok(())
}

#[test]
fn jpeg_and_png_pixels_survive_the_canvas_and_raster_export_path() -> Result<(), TestError> {
    let PixelSize { width, height } = IMAGE.pixel_size();
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        let bytes = encoded(format)?;
        assert_color(&bytes, format)?;
        let tree = SceneTree::with_leaf(scene(bytes)?)?;
        let png = raster_png(tree.at(Duration::ZERO), (width, height), &Fonts::new())?;
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
        let mut surface = gpu.offscreen(IMAGE.pixel_size())?;
        let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
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
    let mut surface = skia_safe::surfaces::raster_n32_premul(IMAGE.dimensions())
        .ok_or(TestError::RasterSurface)?;
    painter.paint_as_leaf(
        surface.canvas(),
        &[DrawCommand::Image {
            image: image.clone(),
            destination: IMAGE.bounds(),
            frame: IMAGE.bounds(),
        }],
        Density::ONE,
    )?;
    assert_eq!(painter.images[&image.id()].1.unique_id(), identity);
    Ok(())
}

#[test]
fn an_encoded_image_takes_the_size_that_its_header_states() -> Result<(), TestError> {
    let PixelSize { width, height } = IMAGE.pixel_size();
    for format in [EncodedImageFormat::JPEG, EncodedImageFormat::PNG] {
        assert_eq!(
            encoded_image(encoded(format)?)?.dimensions(),
            (width, height)
        );
    }
    Ok(())
}
