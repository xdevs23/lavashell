// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::scalar::{scalar, scalars};
use lavashell_core::prelude::*;
use skia_safe::{
    Canvas, IRect, Matrix, Path, PathBuilder, PathFillType, Point as SkPoint, RRect,
    Rect as SkRect, Vector,
};

/// The Skia rectangle of `r`.
///
/// # Errors
/// Fails when a number of `r` lies beyond the range of Skia scalars.
pub fn skrect(r: Rect) -> Result<SkRect, DrawFailure> {
    let [x, y, width, height] = scalars([r.x, r.y, r.width, r.height].map(Dp::get))?;
    Ok(SkRect::from_xywh(x, y, width, height))
}

/// The Skia point of `p`.
///
/// # Errors
/// Fails when a coordinate of `p` lies beyond the range of Skia scalars.
pub fn skpoint(p: Point) -> Result<SkPoint, DrawFailure> {
    let [x, y] = scalars([p.x.get(), p.y.get()])?;
    Ok(SkPoint::new(x, y))
}

pub(crate) fn sklength(length: impl Into<Dp>) -> Result<f32, DrawFailure> {
    scalar(length.into().get())
}

/// The Skia scalar `value` as a length in dp. Every `f32` converts to `f64` exactly.
pub(crate) fn dp_of(value: f32) -> Dp {
    Dp::new(f64::from(value))
}

pub(crate) fn rect_of(r: SkRect) -> Rect {
    Rect::new(
        dp_of(r.left),
        dp_of(r.top),
        dp_of(r.width()),
        dp_of(r.height()),
    )
}

pub(crate) fn rrect(rect: Rect, corners: impl Into<Corners<Radius>>) -> Result<RRect, DrawFailure> {
    let mut radii = [Vector::default(); 4];
    for (radius, corner) in radii.iter_mut().zip(corners.into().fitted(rect).to_array()) {
        let [x, y] = scalars([corner.x.get(), corner.y.get()])?;
        *radius = Vector::new(x, y);
    }
    Ok(RRect::new_rect_radii(skrect(rect)?, &radii))
}

pub(crate) fn rounded(shape: &RoundedRect) -> Result<RRect, DrawFailure> {
    rrect(shape.rect, shape.corners)
}

pub(crate) fn skpath(polygon: &Polygon) -> Result<Path, DrawFailure> {
    let points = (polygon.points().iter())
        .map(|p| skpoint(*p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Path::polygon(&points, true, PathFillType::EvenOdd, None))
}

pub(crate) fn skpath_through(
    start: Point,
    segments: &[Segment],
    closed: bool,
) -> Result<Path, DrawFailure> {
    let mut builder = PathBuilder::new();
    let _chained_builder = builder.move_to(skpoint(start)?);
    for segment in segments {
        let _chained_builder = match *segment {
            Segment::Line(to) => builder.line_to(skpoint(to)?),
            Segment::Quadratic { control, to } => builder.quad_to(skpoint(control)?, skpoint(to)?),
            Segment::Cubic {
                first_control,
                second_control,
                to,
            } => builder.cubic_to(
                skpoint(first_control)?,
                skpoint(second_control)?,
                skpoint(to)?,
            ),
        };
    }
    if closed {
        let _chained_builder = builder.close();
    }
    Ok(builder.detach())
}

pub(crate) fn skmatrix(affine: &Affine) -> Result<Matrix, DrawFailure> {
    let [scale_x, skew_x, skew_y, scale_y] = scalars(affine.linear)?;
    let [translate_x, translate_y] =
        scalars([affine.translation.x.get(), affine.translation.y.get()])?;
    Ok(Matrix::new_all(
        scale_x,
        skew_x,
        translate_x,
        skew_y,
        scale_y,
        translate_y,
        0.0,
        0.0,
        1.0,
    ))
}

/// The most device pixels that one unit of `canvas` spans, or None when its
/// transform collapses every length.
pub(crate) fn device_scale(canvas: &Canvas) -> Option<f32> {
    let scale = canvas.local_to_device_as_3x3().max_scale();
    (scale > 0.0).then_some(scale)
}

/// The longest side of an image that the GPU behind `canvas` can render into and sample, or no
/// bound for a canvas without a GPU.
pub(crate) fn max_image_side(canvas: &Canvas) -> i32 {
    canvas.recording_context().map_or(i32::MAX, |context| {
        context
            .max_texture_size()
            .min(context.max_render_target_size())
    })
}

/// Clips `canvas` to the device pixels `pixels`, or to no pixels when `pixels` is None.
pub(crate) fn clip_device(canvas: &Canvas, pixels: Option<IRect>) {
    let matrix = canvas.local_to_device();
    let _chained_canvas = canvas.reset_matrix();
    let _chained_canvas = canvas.clip_irect(pixels.unwrap_or_default(), None);
    let _chained_canvas = canvas.set_matrix(&matrix);
}
