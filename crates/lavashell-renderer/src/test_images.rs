// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_surfaces::{TestSurface, encoded_pixels};
use crate::{Fonts, Painter, decode_image};
use lavashell_core::prelude::*;
use skia_safe::{AlphaType, ColorSpace, ColorType, EncodedImageFormat, ImageInfo};

pub const IMAGE: TestSurface = TestSurface::new(8, 8);

pub fn encoded(format: EncodedImageFormat) -> Result<Vec<u8>, TestError> {
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

pub fn scene(bytes: Vec<u8>) -> Result<Vec<DrawCommand>, TestError> {
    let image = decode_image(bytes)?;
    let mut view = View::new(lavashell_test_fixtures::facts(), move |ui| {
        let image = image.clone();
        ui.canvas(IMAGE.size(), Modifier::new(), move |canvas| {
            canvas.image(&image, IMAGE.bounds(), ImageFit::Stretch);
        });
    });
    let mut painter = Painter::without_device(&Fonts::new())?;
    let viewport = Viewport::new(IMAGE.size(), Density::ONE);
    crate::test_scenes::framed_commands(&mut view, viewport, &mut painter)
}

pub fn assert_color(bytes: &[u8], format: EncodedImageFormat) -> Result<(), TestError> {
    let pixel = encoded_pixels(bytes, AlphaType::Unpremul)?.at(4, 4);
    for (actual, expected) in pixel.into_iter().zip([48u8, 96, 144, 255]) {
        assert!(actual.abs_diff(expected) <= 3, "{format:?}: {pixel:?}");
    }
    Ok(())
}
