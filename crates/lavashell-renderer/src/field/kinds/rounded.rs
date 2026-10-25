// SPDX-License-Identifier: GPL-3.0-only
use super::{Analytic, Kind, KindFamily, Kinds, ROUNDED, Route, Texels, hash_bits, rect};
use crate::failure::DrawFailure;
use crate::field::program::{Part, uniform_block};
use lavashell_core::prelude::*;
use std::collections::hash_map::DefaultHasher;

uniform_block!(Single {
    bounds: 4,
    radii: 4,
});

impl Single {
    pub(super) fn new(rounded: &RoundedRect) -> Self {
        Self {
            bounds: rect(rounded.rect),
            radii: rounded.fitted_corners().to_array().map(Dp::get),
        }
    }
}

pub(super) fn bounds_and_given_radii(shape: &RoundedRect) -> [[f64; 4]; 2] {
    [rect(shape.rect), shape.corners.to_array().map(Dp::get)]
}

pub(super) struct Rounded;

impl Kind for Rounded {
    type Shape = RoundedRect;
    type Uniforms = Single;

    const NAME: &'static str = "rounded box";
    const PARTS: &'static [Part] = &[ROUNDED, Part::new(include_str!("single.sksl"))];
    const LENS: Option<Part> = Some(Part::new(include_str!("single_lens.sksl")));

    fn fit(
        &self,
        _: &Kinds,
        shape: &RoundedRect,
        _: usize,
    ) -> Result<Single, &dyn Texels<RoundedRect>> {
        Ok(Single::new(shape))
    }

    fn hash(&self, _: &Kinds, shape: &RoundedRect, hasher: &mut DefaultHasher) {
        hash_bits(bounds_and_given_radii(shape).into_iter().flatten(), hasher);
    }

    /// Skia draws a rounded box with one radius for all corners exactly. A box
    /// with different radii goes through the field, which is exact also while the
    /// corners animate.
    fn route(&self, _: &Kinds, shape: &RoundedRect) -> Result<Route, DrawFailure> {
        Ok(match shape.fitted_corners().uniform() {
            Some(_) => Route::Rounded(*shape),
            None => Route::Field,
        })
    }

    fn family<B, C>(programs: &Analytic<B, C>) -> Option<&KindFamily<Self, B, C>> {
        programs.rounded.as_ref()
    }
}
