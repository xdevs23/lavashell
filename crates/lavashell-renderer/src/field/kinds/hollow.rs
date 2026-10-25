// SPDX-License-Identifier: GPL-3.0-only
use super::rounded::{Single, bounds_and_given_radii};
use super::{Analytic, Kind, KindFamily, Kinds, ROUNDED, Route, Texels};
use crate::failure::DrawFailure;
use crate::field::outline::{OutlinePath, OutlinePaths};
use crate::field::program::{Part, uniform_block};
use crate::paint::rounded;
use crate::retention::Frames;
use lavashell_core::shape;
use skia_safe::{Path, PathOp};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::rc::Rc;

uniform_block!(Boxes {
    outer_bounds: 4,
    outer_radii: 4,
    hole_bounds: 4,
    hole_radii: 4,
});

#[derive(Default)]
pub(super) struct Hollow {
    paths: OutlinePaths<Key, Result<Option<Rc<OutlinePath>>, DrawFailure>>,
}

impl Hollow {
    pub fn retain_recent(&mut self, frames: &Frames) {
        self.paths.retain_recent(frames);
    }
}

impl Kind for Hollow {
    type Shape = shape::Hollow;
    type Uniforms = Boxes;

    const NAME: &'static str = "rounded box with a hole";
    const PARTS: &'static [Part] = &[ROUNDED, Part::new(include_str!("hollow.sksl"))];
    const LENS: Option<Part> = None;

    fn fit(
        &self,
        _: &Kinds,
        shape: &shape::Hollow,
        _: usize,
    ) -> Result<Boxes, &dyn Texels<shape::Hollow>> {
        let (outer, hole) = (Single::new(&shape.outer()), Single::new(&shape.hole()));
        Ok(Boxes {
            outer_bounds: outer.bounds,
            outer_radii: outer.radii,
            hole_bounds: hole.bounds,
            hole_radii: hole.radii,
        })
    }

    fn hash(&self, _: &Kinds, shape: &shape::Hollow, hasher: &mut DefaultHasher) {
        Key::of(shape).hash(hasher);
    }

    /// Skia draws the difference of the two boxes exactly, also where the edges
    /// of the boxes meet. The field leaves a seam there.
    fn route(&self, kinds: &Kinds, shape: &shape::Hollow) -> Result<Route, DrawFailure> {
        let path =
            (self.paths).get_or_make(Key::of(shape), kinds.frames(), || difference(shape))?;
        Ok(path.map_or(Route::Field, Route::Path))
    }

    fn family<B, C>(programs: &Analytic<B, C>) -> Option<&KindFamily<Self, B, C>> {
        programs.hollow.as_ref()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    outer: [[u64; 4]; 2],
    hole: [[u64; 4]; 2],
}

impl Key {
    fn of(shape: &shape::Hollow) -> Self {
        let bits = |outline: shape::RoundedRect| {
            bounds_and_given_radii(&outline).map(|part| part.map(f64::to_bits))
        };
        Self {
            outer: bits(shape.outer()),
            hole: bits(shape.hole()),
        }
    }
}

/// The path of the outer box minus the hole. None where the Skia path operations fail. The field then draws the
/// shape.
///
/// # Errors
/// Fails when the boxes lie beyond the range of Skia scalars.
fn difference(shape: &shape::Hollow) -> Result<Option<Rc<OutlinePath>>, DrawFailure> {
    let (outer, hole) = (rounded(&shape.outer())?, rounded(&shape.hole())?);
    let difference = Path::rrect(outer, None).op(&Path::rrect(hole, None), PathOp::Difference);
    Ok(difference.map(|path| Rc::new(OutlinePath::new(path))))
}
