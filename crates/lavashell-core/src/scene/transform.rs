// SPDX-License-Identifier: GPL-3.0-only
use super::Affine;
use crate::geometry::{Point, Rect, Scale};
use crate::units::Dp;

/// An axis-aligned scale followed by a translation. Painting, hit testing and clipping map coordinates through the
/// same transform.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Transform {
    pub scale: Scale,
    pub translation: Point,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        scale: Scale { x: 1.0, y: 1.0 },
        translation: Point::new(Dp::ZERO, Dp::ZERO),
    };

    pub fn translation(offset: Point) -> Self {
        Self {
            translation: offset,
            ..Self::IDENTITY
        }
    }

    /// `None` when `affine` rotates or skews.
    pub fn try_of(affine: Affine) -> Option<Self> {
        let [x, b, c, y] = affine.linear;
        (b == 0.0 && c == 0.0).then_some(Self {
            scale: Scale { x, y },
            translation: affine.translation,
        })
    }

    /// `None` when `affine` rotates or skews, or when it is not finite or not invertible.
    #[cfg(feature = "serde")]
    pub fn checked(affine: Affine) -> Option<Self> {
        Self::try_of(affine).filter(|transform| *transform == transform.saturated())
    }

    pub fn scaled_about(bounds: Rect, scale: f64) -> Self {
        Self::scaled_about_point(bounds.center(), scale)
    }

    pub fn scaled_about_point(origin: Point, scale: f64) -> Self {
        Self {
            scale: Scale { x: scale, y: scale },
            translation: Point::new(origin.x * (1.0 - scale), origin.y * (1.0 - scale)),
        }
        .saturated()
    }

    pub fn moving(from: Point, to: Point, scale: Scale) -> Self {
        Self {
            scale,
            translation: Point::new(to.x - from.x * scale.x, to.y - from.y * scale.y),
        }
        .saturated()
    }

    pub fn rect(self, r: Rect) -> Rect {
        let Self { scale, translation } = self;
        Rect::new(
            finite_length(r.x * scale.x + translation.x),
            finite_length(r.y * scale.y + translation.y),
            finite_length(r.width * scale.x),
            finite_length(r.height * scale.y),
        )
    }

    pub fn point(self, p: Point) -> Point {
        let Self { scale, translation } = self;
        Point::new(
            finite_length(p.x * scale.x + translation.x),
            finite_length(p.y * scale.y + translation.y),
        )
    }

    pub fn inverse(self) -> Self {
        let Self { scale, translation } = self;
        Self {
            scale: Scale {
                x: 1.0 / scale.x,
                y: 1.0 / scale.y,
            },
            translation: Point::new(-translation.x / scale.x, -translation.y / scale.y),
        }
        .saturated()
    }

    pub fn then(self, inner: Self) -> Self {
        let Self { scale, translation } = self;
        Self {
            scale: Scale {
                x: scale.x * inner.scale.x,
                y: scale.y * inner.scale.y,
            },
            translation: translation
                .offset(inner.translation.x * scale.x, inner.translation.y * scale.y),
        }
        .saturated()
    }

    /// Clamps the translation to finite values and the scale to finite values away from zero. A transform composed
    /// through any number of nested nodes can then still be inverted in `f64`.
    fn saturated(self) -> Self {
        let Self { scale, translation } = self;
        Self {
            scale: Scale {
                x: invertible(scale.x),
                y: invertible(scale.y),
            },
            translation: Point::new(finite_length(translation.x), finite_length(translation.y)),
        }
    }

    pub fn affine(self) -> Affine {
        Affine::scale(self.scale.x, self.scale.y).translated(self.translation)
    }
}

fn finite(value: f64) -> f64 {
    value.clamp(-f64::MAX, f64::MAX)
}

fn finite_length(value: Dp) -> Dp {
    Dp::new(finite(value.get()))
}

fn invertible(scale: f64) -> f64 {
    finite(scale).abs().max(f64::MIN_POSITIVE).copysign(scale)
}
