// SPDX-License-Identifier: GPL-3.0-only
use super::FieldTexture;
use super::program::{Bound, Program};
use super::programs::{FieldImage, Measure, Texel};
use crate::failure::DrawFailure;
use crate::paint::{device_scale, skrect};
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, ClipOp, Paint, Shader};

/// A range of signed distances from an outline, where negative distances lie inside the outline. `Below` and `Above`
/// are open on one side.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Range {
    Below(Dp),
    Above(Dp),
    Between { inner: Dp, outer: Dp },
}

impl Range {
    pub fn limits(self, unlimited: (f64, f64)) -> (f64, f64) {
        match self {
            Self::Below(outer) => (unlimited.0, outer.get()),
            Self::Above(inner) => (inner.get(), unlimited.1),
            Self::Between { inner, outer } => (inner.get(), outer.get()),
        }
    }
}

/// The points whose signed distance from an outline lies in `range`, with each
/// distance measured at the point moved back by `offset`. The edges of the band
/// are antialiased over one device pixel, or blurred by a Gaussian of `sigma` dp.
/// `reach` is how far in dp the band extends beyond the bounds of the shape.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Band {
    pub range: Range,
    pub sigma: f64,
    pub offset: Point,
    pub reach: f64,
}

impl Band {
    pub const FILL: Self = Self {
        range: Range::Below(Dp::ZERO),
        sigma: 0.0,
        offset: Point::new(Dp::ZERO, Dp::ZERO),
        reach: 0.0,
    };

    pub fn stroke(stroke: Stroke) -> Self {
        let distances = stroke.distances();
        Self {
            range: Range::Between {
                inner: distances.start,
                outer: distances.end,
            },
            sigma: 0.0,
            offset: Point::new(Dp::ZERO, Dp::ZERO),
            reach: stroke.reach().get(),
        }
    }

    /// The band of `shadow` on a target whose channel values lie `channel_step` apart. An outer shadow covers the
    /// outside of the outline. An inset shadow covers the inside.
    pub fn shadow(shadow: Shadow, channel_step: f64) -> Self {
        let spread = shadow.spread.get();
        Self {
            range: if shadow.inset {
                Range::Above(-spread)
            } else {
                Range::Below(spread)
            },
            sigma: shadow.blur.get().get(),
            offset: shadow.offset.get(),
            reach: shadow.reach(channel_step).get(),
        }
    }

    /// The rectangle around `bounds` that contains the band, one device pixel
    /// wider so that the antialiased edge of the rectangle lies outside the band.
    pub fn area(&self, canvas: &Canvas, bounds: Rect) -> Result<skia_safe::Rect, DrawFailure> {
        let margin = scalar(self.reach + pixel(canvas))?;
        Ok(skrect(bounds)?.with_outset((margin, margin)))
    }
}

/// The size of a device pixel in dp, or one dp where the transform collapses
/// every length and nothing is drawn.
fn pixel(canvas: &Canvas) -> f64 {
    device_scale(canvas).map_or(1.0, |scale| 1.0 / f64::from(scale))
}

pub(crate) struct Coverage<'a> {
    field: Field<'a>,
    bounds: Rect,
    pixel: f64,
}

pub(super) enum Field<'a> {
    Analytic(Bound<'a, Measure>),
    Texture(&'a Program<(Texel, Measure), FieldImage>, FieldTexture),
}

impl<'a> Coverage<'a> {
    pub(super) fn new(canvas: &Canvas, shape: &Shape, field: Field<'a>) -> Self {
        Self {
            field,
            bounds: shape.bounds(),
            pixel: pixel(canvas),
        }
    }

    pub fn clip(&self, canvas: &Canvas, band: Band, op: ClipOp) -> Result<(), DrawFailure> {
        let _chained_canvas = canvas.clip_shader(self.shader(band)?, op);
        Ok(())
    }

    pub fn fill(&self, canvas: &Canvas, band: Band, paint: &Paint) -> Result<(), DrawFailure> {
        let saved = canvas.save();
        let clipped = self.clip(canvas, band, ClipOp::Intersect);
        let filled = clipped.and_then(|()| {
            let _chained_canvas = canvas.draw_rect(band.area(canvas, self.bounds)?, paint);
            Ok(())
        });
        let _chained_canvas = canvas.restore_to_count(saved);
        filled
    }

    fn shader(&self, band: Band) -> Result<Shader, DrawFailure> {
        let (inner, outer) = band.range.limits((0.0, 0.0));
        let (inner_open, outer_open) = match band.range {
            Range::Below(_) => (1.0, 0.0),
            Range::Above(_) => (0.0, 1.0),
            Range::Between { .. } => (0.0, 0.0),
        };
        let measure = Measure {
            inner: [inner],
            outer: [outer],
            inner_open: [inner_open],
            outer_open: [outer_open],
            sigma: [band.sigma],
            pixel: [self.pixel],
            offset: [band.offset.x.get(), band.offset.y.get()],
            saturation: [saturation()],
        };
        match &self.field {
            Field::Analytic(bound) => bound.evaluate(&measure, ()),
            Field::Texture(program, texture) => {
                let texel = Texel {
                    texel: [texture.texel()],
                };
                let field = texture.shader()?;
                program.evaluate(&(texel, measure), FieldImage { field })
            }
        }
    }
}

/// The number of standard deviations beyond which the tail `exp(-z * z)` of the blurred coverage falls below the
/// rounding unit of an `f32`, the float type of the shader. Beyond that point the coverage rounds to exactly 0 or 1.
fn saturation() -> f64 {
    let rounding_unit = f64::from(f32::EPSILON) / 2.0;
    (-rounding_unit.ln()).sqrt()
}
