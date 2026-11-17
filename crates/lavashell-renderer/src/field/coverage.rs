// SPDX-License-Identifier: GPL-3.0-only
use super::FieldTexture;
use super::program::{Bound, Program};
use super::programs::{FieldImage, Measure, Texel};
use crate::failure::DrawFailure;
use crate::paint::{min_device_scale, skrect};
use crate::scalar::scalar;
use lavashell_core::prelude::*;
use skia_safe::{Canvas, ClipOp, Paint, Shader};

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

    pub fn area(&self, canvas: &Canvas, bounds: Rect) -> Result<skia_safe::Rect, DrawFailure> {
        let margin = scalar(self.reach + pixel(canvas)?)?;
        Ok(skrect(bounds)?.with_outset((margin, margin)))
    }
}

fn pixel(canvas: &Canvas) -> Result<f64, DrawFailure> {
    let scale = min_device_scale(canvas).ok_or(DrawFailure::SingularTransform)?;
    Ok(1.0 / f64::from(scale))
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
    pub(super) fn new(
        canvas: &Canvas,
        shape: &Shape,
        field: Field<'a>,
    ) -> Result<Self, DrawFailure> {
        Ok(Self {
            field,
            bounds: shape.bounds(),
            pixel: pixel(canvas)?,
        })
    }

    pub fn clip(&self, canvas: &Canvas, band: Band, op: ClipOp) -> Result<(), DrawFailure> {
        let _canvas_returned_for_chaining = canvas.clip_shader(self.shader(band)?, op);
        Ok(())
    }

    pub fn fill(&self, canvas: &Canvas, band: Band, paint: &Paint) -> Result<(), DrawFailure> {
        let saved = canvas.save();
        let clipped = self.clip(canvas, band, ClipOp::Intersect);
        let filled = clipped.and_then(|()| {
            let _canvas_returned_for_chaining =
                canvas.draw_rect(band.area(canvas, self.bounds)?, paint);
            Ok(())
        });
        let _canvas_returned_for_chaining = canvas.restore_to_count(saved);
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

fn saturation() -> f64 {
    let rounding_unit = f64::from(f32::EPSILON) / 2.0;
    (-rounding_unit.ln()).sqrt()
}
