// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::prelude::*;
use std::sync::Arc;

/// An image of the encoded `bytes`, decoded into raster pixels right away. Decoding first proves that Skia can read
/// the image before the image goes into a display list.
///
/// # Errors
/// Fails when Skia cannot decode the image.
pub fn decode_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let decoded = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&bytes))
        .and_then(|image| image.make_raster_image(None, None))
        .ok_or(Error::DecodeImage)?;
    let width = u32::try_from(decoded.width()).map_err(Error::DecodedImageSize)?;
    let height = u32::try_from(decoded.height()).map_err(Error::DecodedImageSize)?;
    Ok(Image::encoded(bytes, width, height).with_backend_data(decoded))
}

/// An image of the encoded `bytes`, of the size given in their header. Its pixels are not
/// decoded here but when the image is drawn.
///
/// # Errors
/// Fails when Skia cannot read the header of the image.
pub fn encoded_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let header = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&bytes))
        .ok_or(Error::ReadImageHeader)?;
    let width = u32::try_from(header.width()).map_err(Error::ImageHeaderSize)?;
    let height = u32::try_from(header.height()).map_err(Error::ImageHeaderSize)?;
    Ok(Image::encoded(bytes, width, height))
}

/// Paints `tree` on a CPU raster surface of `dimensions` pixels and encodes the result as PNG, for export to a file
/// or the clipboard.
///
/// # Errors
/// Fails when Skia cannot allocate the surfaces of the export or encode the PNG, or the painter cannot start.
pub fn raster_png(
    tree: Presented<'_>,
    dimensions: (u32, u32),
    fonts: &crate::Fonts,
) -> Result<Vec<u8>, Error> {
    let (Ok(width), Ok(height)) = (i32::try_from(dimensions.0), i32::try_from(dimensions.1)) else {
        return Err(Error::RasterSize(dimensions));
    };
    let info = skia_safe::ImageInfo::new(
        (width, height),
        skia_safe::ColorType::RGBAF16,
        skia_safe::AlphaType::Premul,
        crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
    );
    let mut surface = skia_safe::surfaces::raster(&info, None, None).ok_or(Error::RasterSurface)?;
    let _chained_canvas = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    crate::Painter::new(fonts, crate::UniformRange::VULKAN_REQUIRED)?.paint_scene(
        surface.canvas(),
        tree,
        Density::ONE,
    );
    let mut export = skia_safe::surfaces::raster(
        &skia_safe::ImageInfo::new_n32_premul((width, height), skia_safe::ColorSpace::new_srgb()),
        None,
        None,
    )
    .ok_or(Error::PngSurface)?;
    let _chained_canvas = export.canvas().clear(skia_safe::Color::TRANSPARENT);
    let _chained_canvas = export
        .canvas()
        .draw_image(surface.image_snapshot(), (0, 0), None);
    export
        .image_snapshot()
        .encode(None, skia_safe::EncodedImageFormat::PNG, None)
        .map(|data| data.as_bytes().to_vec())
        .ok_or(Error::EncodePng)
}
