// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::{DrawFailure, FailedDraw};
use crate::field::{Band, Coverage, OutlinePath, Range, Route};
use crate::paint::{rect_of, rounded, sklength, skpoint};
use crate::resources::PaintResources;
use crate::scalar::{scalar, scalars};
use lavashell_core::prelude::*;
use skia_safe::{
    Canvas, ClipOp, Paint, PaintJoin, PaintStyle, Path, PathBuilder, PathOp, RRect, image_filters,
};
use std::rc::Rc;

pub(crate) struct Silhouette<'a> {
    resources: &'a PaintResources,
    shape: &'a Shape,
    reach: f64,
    form: Form<'a>,
}

enum Form<'a> {
    Rounded(RoundedRect),
    Path(Rc<OutlinePath>),
    Field(Coverage<'a>),
}

impl<'a> Silhouette<'a> {
    /// The silhouette of `shape`, drawn as a rounded rectangle, as a path or through its field. Returns None when the
    /// shape is drawn through its field and no part of the shape is visible.
    pub fn new(
        canvas: &Canvas,
        resources: &'a PaintResources,
        shape: &'a Shape,
        reach: f64,
    ) -> Result<Option<Self>, FailedDraw> {
        let fields = &resources.fields;
        let form = match fields.route(shape)? {
            Route::Rounded(rounded) => Form::Rounded(rounded),
            Route::Path(path) => Form::Path(path),
            Route::Field => match fields.coverage(canvas, shape, reach)? {
                Some(coverage) => Form::Field(coverage),
                None => return Ok(None),
            },
        };
        Ok(Some(Self {
            resources,
            shape,
            reach,
            form,
        }))
    }

    pub fn fill(&self, canvas: &Canvas, paint: &Paint) -> Result<(), DrawFailure> {
        match &self.form {
            Form::Rounded(shape) => {
                let _chained_canvas = canvas.draw_rrect(rounded(shape)?, paint);
            }
            Form::Path(outline) => self.tiles().draw(canvas, &outline.path, paint)?,
            Form::Field(coverage) => coverage.fill(canvas, Band::FILL, paint)?,
        }
        Ok(())
    }

    pub fn clip(&self, canvas: &Canvas, op: ClipOp) -> Result<(), DrawFailure> {
        match &self.form {
            Form::Rounded(shape) => {
                let _chained_canvas = canvas.clip_rrect(rounded(shape)?, op, true);
            }
            Form::Path(outline) => self.tiles().clip(canvas, &outline.path, op)?,
            Form::Field(coverage) => coverage.clip(canvas, Band::FILL, op)?,
        }
        Ok(())
    }

    /// Paints `band` around the outline. Where Skia draws the outline, the image filter of the paint blurs the band.
    /// Where the field draws the outline, the field blurs its coverage. When the Skia path operations fail on the
    /// path of the outline, the band is painted from the field of the shape.
    pub fn band(&self, canvas: &Canvas, band: Band, paint: &Paint) -> Result<(), FailedDraw> {
        let mut blurred = paint.clone();
        let sigma = scalar(band.sigma)?;
        if sigma > 0.0 {
            let _chained_paint = blurred.set_image_filter(image_filters::blur(
                (sigma, sigma),
                skia_safe::TileMode::Decal,
                None,
                None,
            ));
        }
        match &self.form {
            Form::Rounded(shape) => {
                let shape = shape.translated(band.offset);
                let outset = |distance: Dp| rounded(&shape.outset(distance));
                let _chained_canvas = match band.range {
                    Range::Below(outer) => canvas.draw_rrect(outset(outer)?, &blurred),
                    Range::Above(inner) => canvas.draw_drrect(
                        RRect::new_rect(band.area(canvas, shape.rect)?),
                        outset(inner)?,
                        &blurred,
                    ),
                    Range::Between { inner, outer } => {
                        let _chained_paint = blurred
                            .set_style(PaintStyle::Stroke)
                            .set_stroke_width(sklength(outer - inner)?);
                        canvas.draw_rrect(outset((inner + outer) / 2.0)?, &blurred)
                    }
                };
            }
            Form::Path(outline) => {
                let frames = self.resources.fields.frames();
                let edges = BandEdges::new(canvas, &outline.path, band)?;
                let region = outline.region(frames, edges.numbers(), |path| edges.path(path));
                if let Some(region) = region {
                    self.tiles().draw(canvas, &region, &blurred)?;
                } else {
                    let fields = &self.resources.fields;
                    if let Some(coverage) = fields.coverage(canvas, self.shape, self.reach)? {
                        coverage.fill(canvas, band, paint)?;
                    }
                }
            }
            Form::Field(coverage) => coverage.fill(canvas, band, paint)?,
        }
        Ok(())
    }

    fn tiles(&self) -> std::cell::RefMut<'_, crate::raster::RasterCache> {
        self.resources.tiles.borrow_mut()
    }
}

/// Clips `canvas` to the inside of `shape`. When the shape is drawn through its field and no part of it is visible,
/// the clip removes everything.
pub(crate) fn clip_inside(
    canvas: &Canvas,
    resources: &PaintResources,
    shape: &Shape,
) -> Result<(), FailedDraw> {
    if let Some(silhouette) = Silhouette::new(canvas, resources, shape, 0.0)? {
        (silhouette.clip(canvas, ClipOp::Intersect)).map_err(FailedDraw::from)
    } else {
        let _chained_canvas = canvas.clip_rect(skia_safe::Rect::new_empty(), None, false);
        Ok(())
    }
}

/// A band around a path, with its numbers converted to the `f32` used by Skia.
struct BandEdges {
    range: Range,
    /// The distances of the inner and the outer edge from the path, infinite where the band has no such edge.
    limits: [f32; 2],
    area: skia_safe::Rect,
    offset: skia_safe::Point,
}

impl BandEdges {
    fn new(canvas: &Canvas, path: &Path, band: Band) -> Result<Self, DrawFailure> {
        let (inner, outer) = band.range.limits((f64::NEG_INFINITY, f64::INFINITY));
        Ok(Self {
            range: band.range,
            limits: scalars([inner, outer])?,
            area: band.area(canvas, rect_of(*path.bounds()))?,
            offset: skpoint(band.offset)?,
        })
    }

    /// The bits of every number that changes the region of a band, used as the key of the stored region.
    fn numbers(&self) -> [u64; 8] {
        let [inner, outer] = self.limits;
        let area = self.area;
        [
            inner,
            outer,
            self.offset.x,
            self.offset.y,
            area.left,
            area.top,
            area.right,
            area.bottom,
        ]
        .map(|number| f64::from(number).to_bits())
    }

    fn path(&self, path: &Path) -> Option<Path> {
        let difference = |outer: Path, inner: Path| outer.op(&inner, PathOp::Difference);
        let [inner, outer] = self.limits;
        let region = match self.range {
            Range::Below(_) => offset(path, outer)?,
            Range::Above(_) => difference(Path::rect(self.area, None), offset(path, inner)?)?,
            Range::Between { .. } => difference(offset(path, outer)?, offset(path, inner)?)?,
        };
        Some(region.with_offset(self.offset))
    }
}

fn offset(path: &Path, amount: f32) -> Option<Path> {
    if amount == 0.0 {
        return Some(path.clone());
    }
    let mut stroke = Paint::default();
    let _chained_paint = stroke
        .set_style(PaintStyle::Stroke)
        .set_stroke_width(2.0 * amount.abs())
        .set_stroke_join(PaintJoin::Round);
    let mut band = PathBuilder::new();
    if !skia_safe::path_utils::fill_path_with_paint(path, &stroke, &mut band, None, None) {
        return None;
    }
    let op = if amount > 0.0 {
        PathOp::Union
    } else {
        PathOp::Difference
    };
    path.op(&band.detach(), op)
}

#[cfg(test)]
mod hole_tests;
#[cfg(test)]
mod tests;
