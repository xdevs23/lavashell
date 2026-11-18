// SPDX-License-Identifier: GPL-3.0-only

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
#[cfg(any(test, feature = "test-support"))]
pub mod test_shapes;
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

const SEGMENT_FLOATS: usize = 4;

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
