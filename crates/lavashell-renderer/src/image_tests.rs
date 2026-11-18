// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_images::{IMAGE, assert_color, encoded, scene};
use crate::{Fonts, Painter, decode_image, encoded_image, raster_png};
use lavashell_core::prelude::*;
use skia_safe::EncodedImageFormat;
use std::time::Duration;

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
fn decoded_cpu_asset_is_shared_with_the_painter_without_a_second_decode() -> Result<(), TestError> {
    let image = decode_image(encoded(EncodedImageFormat::JPEG)?)?;
    let decoded = image
        .backend_data::<skia_safe::Image>()
        .ok_or(TestError::Missing("Skia image in the decoded image"))?;
    assert!(!decoded.is_texture_backed());
    let identity = decoded.unique_id();
    let mut painter = Painter::without_device(&Fonts::new())?;
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
