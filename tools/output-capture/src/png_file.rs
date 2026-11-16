// SPDX-License-Identifier: GPL-3.0-only
use crate::error::SessionError;
use crate::frame::CapturedFrame;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const BYTES_PER_PIXEL: usize = 4;

pub fn write_png(pixels: &[u8], frame: CapturedFrame, path: &Path) -> Result<(), SessionError> {
    let layout = frame.layout;
    let length = |value: u32| usize::try_from(value).map_err(SessionError::FrameOutOfRange);
    let row_length =
        (length(layout.width)?.checked_mul(BYTES_PER_PIXEL)).ok_or(SessionError::FrameTooLarge)?;
    let mut rows: Vec<&[u8]> = (pixels.chunks_exact(length(layout.stride)?))
        .map(|row| &row[..row_length])
        .collect();
    if frame.inverted {
        rows.reverse();
    }
    let file = File::create(path).map_err(SessionError::CreateImage)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), layout.width, layout.height);
    encoder.set_color(layout.format.png_color());
    encoder.set_depth(png::BitDepth::Eight);
    let mut header = encoder.write_header().map_err(SessionError::EncodeImage)?;
    let mut image = header.stream_writer().map_err(SessionError::EncodeImage)?;
    let mut channels = Vec::new();
    for row in rows {
        channels.clear();
        for pixel in row.as_chunks::<BYTES_PER_PIXEL>().0 {
            layout.format.append_png_channels(*pixel, &mut channels);
        }
        image
            .write_all(&channels)
            .map_err(|error| SessionError::EncodeImage(error.into()))?;
    }
    image.finish().map_err(SessionError::EncodeImage)
}

#[cfg(test)]
mod tests;
