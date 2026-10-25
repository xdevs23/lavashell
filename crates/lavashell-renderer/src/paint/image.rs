// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests;

use super::{Painter, max_image_side, skrect};
use crate::failure::DrawFailure;
use crate::scalar::scalars;
use lavashell_core::prelude::*;
use skia_safe::canvas::SrcRectConstraint;
use skia_safe::{
    Canvas, FilterMode, IRect, ISize, Matrix, MipmapMode, Paint, Rect as SkRect, SamplingOptions,
};
use std::collections::hash_map::Entry;

impl Painter {
    pub(super) fn image(
        &mut self,
        canvas: &Canvas,
        image: &Image,
        destination: Rect,
        frame: Rect,
    ) -> Result<(), DrawFailure> {
        let decoded = match self.images.entry(image.id()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert((
                std::sync::Arc::downgrade(image.data()),
                pixels(image, canvas)?,
            )),
        };
        let decoded = &decoded.1;
        let decoded_size = decoded.dimensions();
        let decoded_size = (
            u32::try_from(decoded_size.width),
            u32::try_from(decoded_size.height),
        );
        let (width, height) = image.dimensions();
        if decoded_size != (Ok(width), Ok(height)) {
            return Err(DrawFailure::ImageSize);
        }
        draw_framed(canvas, decoded, decoded.bounds(), destination, frame)
    }
}

/// The raster pixels of `image`, decoded when the image contains only the encoded bytes and has
/// no more pixels than the largest square image supported by the GPU behind `canvas`.
fn pixels(image: &Image, canvas: &Canvas) -> Result<skia_safe::Image, DrawFailure> {
    if let Some(pixels) = image.backend_data::<skia_safe::Image>() {
        return Ok(pixels.clone());
    }
    if !within_largest_image(image.dimensions(), max_image_side(canvas)) {
        return Err(DrawFailure::ImagePixels);
    }
    skia_safe::Image::from_encoded(skia_safe::Data::new_copy(image.bytes()))
        .and_then(|image| image.make_raster_image(None, None))
        .ok_or(DrawFailure::ImageUndecodable)
}

/// Whether an image of `width` by `height` pixels has at most as many pixels as a square image of
/// `max_side` pixels.
fn within_largest_image((width, height): (u32, u32), max_side: i32) -> bool {
    i128::from(width) * i128::from(height) <= i128::from(max_side).pow(2)
}

pub(super) fn draw_framed(
    canvas: &Canvas,
    pixels: &skia_safe::Image,
    area: IRect,
    destination: Rect,
    frame: Rect,
) -> Result<(), DrawFailure> {
    let Some(source) = source(frame, destination.size(), area.size())? else {
        return Ok(());
    };
    let source = source.with_offset(SkRect::from(area).tl());
    // The box past the edges of `area` is empty, like the area past the edges of an image drawn
    // by Skia.
    let mut visible = source;
    if !visible.intersect(SkRect::from(area)) {
        return Ok(());
    }
    let Some(source_to_box) = Matrix::rect_2_rect(source, skrect(destination)?, None) else {
        return Ok(());
    };
    let _chained_canvas = canvas.draw_image_rect_with_sampling_options(
        pixels,
        Some((&visible, SrcRectConstraint::Strict)),
        source_to_box.map_rect(visible).0,
        SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        &Paint::default(),
    );
    Ok(())
}

/// The rectangle of image pixels shown in a box of `box_size` when the image covers `frame` relative to the box.
/// Where the box extends past the image, the box shows nothing. A frame or an image with a zero or non-finite size
/// gives `None`, so nothing is drawn.
fn source(frame: Rect, box_size: Size, image_size: ISize) -> Result<Option<SkRect>, DrawFailure> {
    let pixel_width = frame.width / f64::from(image_size.width);
    let pixel_height = frame.height / f64::from(image_size.height);
    let [x, y, width, height] = scalars([
        -frame.x / pixel_width,
        -frame.y / pixel_height,
        box_size.width / pixel_width,
        box_size.height / pixel_height,
    ])?;
    let source = SkRect::from_xywh(x, y, width, height);
    Ok((source.is_finite() && !source.is_empty()).then_some(source))
}
