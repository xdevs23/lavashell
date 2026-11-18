// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::prelude::*;
use lavashell_core::surface::SurfaceIds;
use skia_safe::png_encoder::{self, FilterFlag};
use skia_safe::{Canvas, Data, ISize, images};
use std::sync::Arc;

pub struct Pixels {
    dimensions: ISize,
    bytes: Vec<u8>,
}

pub fn decode_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let decoded = skia_safe::Image::from_encoded(Data::new_copy(&bytes))
        .and_then(|image| image.make_raster_image(None, None))
        .ok_or(Error::DecodeImage)?;
    let width = u32::try_from(decoded.width()).map_err(Error::DecodedImageSize)?;
    let height = u32::try_from(decoded.height()).map_err(Error::DecodedImageSize)?;
    Ok(Image::encoded(bytes, width, height).with_backend_data(decoded))
}

pub fn encoded_image(bytes: impl Into<Arc<[u8]>>) -> Result<Image, Error> {
    let bytes = bytes.into();
    let header =
        skia_safe::Image::from_encoded(Data::new_copy(&bytes)).ok_or(Error::ReadImageHeader)?;
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
    let info = crate::color::working_image_info((width, height))?;
    let image_surface = SurfaceIds::default()
        .next_id()
        .map_err(Error::NumberImageSurface)?;
    let mut surface = skia_safe::surfaces::raster(&info, None, None).ok_or(Error::RasterSurface)?;
    let _canvas_returned_for_chaining = surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    crate::Painter::without_device(fonts)?.paint_scene(
        surface.canvas(),
        image_surface,
        tree,
        Density::ONE,
    )?;
    let export_info = png_export_info(ISize::new(width, height));
    let mut export =
        skia_safe::surfaces::raster(&export_info, None, None).ok_or(Error::PngSurface)?;
    let _canvas_returned_for_chaining = export.canvas().clear(skia_safe::Color::TRANSPARENT);
    let _canvas_returned_for_chaining =
        export
            .canvas()
            .draw_image(surface.image_snapshot(), (0, 0), None);
    Pixels::read(export.canvas())?.png()
}

#[must_use]
pub fn png_export_info(dimensions: ISize) -> skia_safe::ImageInfo {
    skia_safe::ImageInfo::new_n32_premul(dimensions, skia_safe::ColorSpace::new_srgb())
}

const PNG_ROW_FILTER_FOR_SPEED: FilterFlag = FilterFlag::SUB;

const PNG_ZLIB_LEVEL_FOR_SPEED: i32 = 3;

fn fast_png_options() -> png_encoder::Options {
    let mut options = png_encoder::Options::default();
    options.filter_flags = PNG_ROW_FILTER_FOR_SPEED;
    options.z_lib_level = PNG_ZLIB_LEVEL_FOR_SPEED;
    options
}

impl Pixels {
    pub fn read(canvas: &Canvas) -> Result<Self, Error> {
        let dimensions = canvas.image_info().dimensions();
        let info = png_export_info(dimensions);
        let mut bytes = vec![0; info.compute_min_byte_size()];
        if !canvas.read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0)) {
            return Err(Error::ReadPixels);
        }
        Ok(Self { dimensions, bytes })
    }

    pub fn png(&self) -> Result<Vec<u8>, Error> {
        let info = png_export_info(self.dimensions);
        let data = Data::new_copy(&self.bytes);
        let image =
            images::raster_from_data(&info, data, info.min_row_bytes()).ok_or(Error::EncodePng)?;
        png_encoder::encode_image(None, &image, &fast_png_options())
            .map(|encoded| encoded.as_bytes().to_vec())
            .ok_or(Error::EncodePng)
    }
}
