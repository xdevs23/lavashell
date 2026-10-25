// SPDX-License-Identifier: GPL-3.0-only
//! Signed distance fields of shapes, computed on the GPU. When the geometry of a shape fits into the uniforms of the
//! field program for its kind, that program evaluates the field at every pixel. The field of every other shape is
//! drawn into a field texture first.

use lavashell_core::shape::Shape;
use std::borrow::Cow;
use std::ops::Deref;

mod coverage;
mod family;
mod frame;
mod kinds;
mod outline;
mod program;
mod programs;
mod room;
mod sksl;
#[cfg(test)]
pub(crate) mod test_shapes;
#[cfg(test)]
mod tests;
mod texture;

pub(crate) use coverage::{Band, Coverage, Range};
pub(crate) use family::Pass;
pub(crate) use frame::Fields;
pub(crate) use kinds::{Analytic, Route};
pub(crate) use outline::OutlinePath;
pub(crate) use program::{
    Children, Layout, Part, Program, Uniforms, children, smoothing, uniform_block,
};
pub use room::{FieldRoom, UniformRange};
use texture::FieldTexture;
pub(crate) use texture::{Content, TexelPixels};

/// The floats of one contour segment: the x and y of its start and of its end (`segment.xy` and `segment.zw`).
const SEGMENT_FLOATS: usize = 4;

/// A shape with every hole filled. Only such a shape has a lens field. A hole
/// removes glass, but the glass around the hole bends light as the filled shape does.
pub(crate) struct Solid<'a>(Cow<'a, Shape>);

impl<'a> Solid<'a> {
    pub fn of(shape: &'a Shape) -> Self {
        Self(shape.without_holes())
    }

    pub fn into_shape(self) -> Shape {
        self.0.into_owned()
    }
}

impl Deref for Solid<'_> {
    type Target = Shape;

    fn deref(&self) -> &Shape {
        &self.0
    }
}

pub(crate) const LENS_TEXTURE: Part = Part::new(include_str!("field/lens_texture.sksl"));
