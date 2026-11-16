// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceIds;
use std::sync::Arc;

pub fn decode_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let decoded = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&bytes))
        .and_then(|image| image.make_raster_image(None, None))
        .ok_or(Error::DecodeImage)?;
    let width = u32::try_from(decoded.width()).map_err(Error::DecodedImageSize)?;
    let height = u32::try_from(decoded.height()).map_err(Error::DecodedImageSize)?;
    Ok(Image::encoded(bytes, width, height).with_backend_data(decoded))
}

pub fn encoded_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let header = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&bytes))
        .ok_or(Error::ReadImageHeader)?;
    let width = u32::try_from(header.width()).map_err(Error::ImageHeaderSize)?;
    let height = u32::try_from(header.height()).map_err(Error::ImageHeaderSize)?;
    Ok(Image::encoded(bytes, width, height))
}

pub fn raster_png(
    tree: Presented<'_>,
    dimensions: (u32, u32),
    fonts: &crate::Fonts,
) -> Result<Vec<u8>, Error> {
    let (Ok(width), Ok(height)) = (i32::try_from(dimensions.0), i32::try_from(dimensions.1)) else {
        return Err(Error::RasterSize {
            width: dimensions.0,
            height: dimensions.1,
        });
    };
    let info = skia_safe::ImageInfo::new(
        (width, height),
        skia_safe::ColorType::RGBAF16,
        skia_safe::AlphaType::Premul,
        crate::color::working_color_space().ok_or(Error::WorkingColorSpace)?,
    );
    let image_surface = SurfaceIds::default()
        .next_id()
        .map_err(Error::NumberImageSurface)?;
    let mut surface = skia_safe::surfaces::raster(&info, None, None).ok_or(Error::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    crate::Painter::new(fonts, crate::UniformRange::VULKAN_REQUIRED)?.paint_scene(
        surface.canvas(),
        image_surface,
        tree,
        Density::ONE,
    );
    let export_info = png_export_info(skia_safe::ISize::new(width, height));
    let mut export =
        skia_safe::surfaces::raster(&export_info, None, None).ok_or(Error::PngSurface)?;
    let _canvas_returned_for_chaining = export.canvas().clear(skia_safe::Color::TRANSPARENT);
    let _canvas_returned_for_chaining =
        export
            .canvas()
            .draw_image(surface.image_snapshot(), (0, 0), None);
    export
        .image_snapshot()
        .encode(None, skia_safe::EncodedImageFormat::PNG, None)
        .map(|data| data.as_bytes().to_vec())
        .ok_or(Error::EncodePng)
}

pub(crate) fn png_export_info(dimensions: skia_safe::ISize) -> skia_safe::ImageInfo {
    skia_safe::ImageInfo::new_n32_premul(dimensions, skia_safe::ColorSpace::new_srgb())
}
