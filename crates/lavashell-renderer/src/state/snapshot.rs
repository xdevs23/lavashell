// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::image::png_export_info;
use skia_safe::{Canvas, Data, EncodedImageFormat, ISize, images};

/// The whole frame painted into a surface, before the surface presents it.
pub struct Snapshot<'a> {
    canvas: &'a Canvas,
}

/// The pixels of a painted frame, in the format of every PNG export.
pub struct Pixels {
    dimensions: ISize,
    bytes: Vec<u8>,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(canvas: &'a Canvas) -> Self {
        Self { canvas }
    }

    /// The pixels of the frame, read in the format of every PNG export.
    ///
    /// # Errors
    /// Fails when Skia cannot read the pixels of the frame.
    pub fn pixels(&self) -> Result<Pixels, Error> {
        let dimensions = self.canvas.image_info().dimensions();
        let info = png_export_info(dimensions);
        let mut bytes = vec![0; info.compute_min_byte_size()];
        if !self
            .canvas
            .read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0))
        {
            return Err(Error::ReadPaintedFrame);
        }
        Ok(Pixels { dimensions, bytes })
    }
}

impl Pixels {
    /// The pixels encoded as PNG.
    ///
    /// # Errors
    /// Fails when Skia cannot encode the PNG.
    pub fn png(&self) -> Result<Vec<u8>, Error> {
        let info = png_export_info(self.dimensions);
        let data = Data::new_copy(&self.bytes);
        let image =
            images::raster_from_data(&info, data, info.min_row_bytes()).ok_or(Error::EncodePng)?;
        (image.encode(None, EncodedImageFormat::PNG, None))
            .map(|encoded| encoded.as_bytes().to_vec())
            .ok_or(Error::EncodePng)
    }
}
