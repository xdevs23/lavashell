// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::image::png_export_info;
use skia_safe::{Canvas, Data, EncodedImageFormat, images};

pub struct Snapshot<'a> {
    canvas: &'a Canvas,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(canvas: &'a Canvas) -> Self {
        Self { canvas }
    }

    pub fn png(&self) -> Result<Vec<u8>, Error> {
        let info = png_export_info(self.canvas.image_info().dimensions());
        let row_bytes = info.min_row_bytes();
        let mut pixels = vec![0; info.compute_min_byte_size()];
        if !self
            .canvas
            .read_pixels(&info, &mut pixels, row_bytes, (0, 0))
        {
            return Err(Error::ReadPaintedFrame);
        }
        let image = images::raster_from_data(&info, Data::new_copy(&pixels), row_bytes)
            .ok_or(Error::EncodePng)?;
        (image.encode(None, EncodedImageFormat::PNG, None))
            .map(|encoded| encoded.as_bytes().to_vec())
            .ok_or(Error::EncodePng)
    }
}
