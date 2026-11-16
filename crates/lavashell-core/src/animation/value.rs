// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Insets, Point, Rect, Size};
use crate::output::finest_step;
use crate::scene::Affine;
use crate::shape::{Corners, RoundedRect};
use crate::units::{Dp, ScaleFactor};

#[cfg(feature = "serde")]
mod encoding;

pub trait VectorSpace: Clone + PartialEq + 'static {
    fn zero() -> Self;

    #[must_use]
    fn add(&self, rhs: &Self) -> Self;

    #[must_use]
    fn sub(&self, rhs: &Self) -> Self;

    #[must_use]
    fn scale(&self, factor: f64) -> Self;

    #[must_use]
    fn saturating_scale(&self, factor: f64) -> Self;

    fn max_abs(&self) -> f64;

    fn finite(&self) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector<const N: usize>(pub [f64; N]);

impl<const N: usize> VectorSpace for Vector<N> {
    fn zero() -> Self {
        Self([0.0; N])
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + rhs.0[i]))
    }

    fn sub(&self, rhs: &Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] - rhs.0[i]))
    }

    fn scale(&self, factor: f64) -> Self {
        Self(self.0.map(|v| v * factor))
    }

    fn saturating_scale(&self, factor: f64) -> Self {
        Self(self.0.map(|v| (v * factor).clamp(-f64::MAX, f64::MAX)))
    }

    fn max_abs(&self) -> f64 {
        self.0.iter().fold(0.0_f64, |a, b| a.max(b.abs()))
    }

    fn finite(&self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
}

impl<V: VectorSpace, const N: usize> VectorSpace for [V; N] {
    fn zero() -> Self {
        std::array::from_fn(|_| V::zero())
    }

    fn add(&self, rhs: &Self) -> Self {
        std::array::from_fn(|i| self[i].add(&rhs[i]))
    }

    fn sub(&self, rhs: &Self) -> Self {
        std::array::from_fn(|i| self[i].sub(&rhs[i]))
    }

    fn scale(&self, factor: f64) -> Self {
        std::array::from_fn(|i| self[i].scale(factor))
    }

    fn saturating_scale(&self, factor: f64) -> Self {
        std::array::from_fn(|i| self[i].saturating_scale(factor))
    }

    fn max_abs(&self) -> f64 {
        self.iter().fold(0.0_f64, |a, b| a.max(b.max_abs()))
    }

    fn finite(&self) -> bool {
        self.iter().all(VectorSpace::finite)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trajectory<V> {
    pub position: V,
    pub velocity: V,
    pub end: V,
}

impl<V: VectorSpace> Trajectory<V> {
    pub(crate) fn at_rest(position: V) -> Self {
        Self {
            end: position.clone(),
            velocity: V::zero(),
            position,
        }
    }
}

impl<V> FromIterator<Trajectory<V>> for Trajectory<Vec<V>> {
    fn from_iter<I: IntoIterator<Item = Trajectory<V>>>(each: I) -> Self {
        let mut list = Trajectory {
            position: Vec::new(),
            velocity: Vec::new(),
            end: Vec::new(),
        };
        for Trajectory {
            position,
            velocity,
            end,
        } in each
        {
            list.position.push(position);
            list.velocity.push(velocity);
            list.end.push(end);
        }
        list
    }
}

impl<V> Trajectory<V> {
    pub(crate) fn map<W>(&self, part: impl Fn(&V) -> W) -> Trajectory<W> {
        Trajectory {
            position: part(&self.position),
            velocity: part(&self.velocity),
            end: part(&self.end),
        }
    }
}

pub trait Animatable: Clone + PartialEq + 'static {
    type Vector: VectorSpace;

    fn vector(&self) -> Self::Vector;

    fn visible_step(pixel: Dp) -> f64;

    #[must_use]
    fn at(&self, vector: Self::Vector) -> Self;

    fn retarget(
        &self,
        _previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        Trajectory {
            end: self.vector(),
            ..current
        }
    }

    #[must_use]
    fn spanning(&self, _previous: &Self) -> Self {
        self.clone()
    }
}

pub trait Composite: Clone + PartialEq + 'static {
    type Parts: Animatable;

    fn parts(&self) -> Self::Parts;

    #[must_use]
    fn with_parts(&self, parts: Self::Parts) -> Self;
}

impl<T: Composite> Animatable for T {
    type Vector = <T::Parts as Animatable>::Vector;

    fn vector(&self) -> Self::Vector {
        self.parts().vector()
    }

    fn visible_step(pixel: Dp) -> f64 {
        T::Parts::visible_step(pixel)
    }

    fn at(&self, vector: Self::Vector) -> Self {
        self.with_parts(self.parts().at(vector))
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        self.parts().retarget(&previous.parts(), current)
    }

    fn spanning(&self, previous: &Self) -> Self {
        self.with_parts(self.parts().spanning(&previous.parts()))
    }
}

macro_rules! value {
    ($ty:ty, $n:expr, $($field:ident),+) => {
        impl Animatable for $ty {
            type Vector = Vector<$n>;
            fn vector(&self) -> Self::Vector { Vector([$(self.$field.get()),+]) }
            fn visible_step(pixel: Dp) -> f64 { Dp::visible_step(pixel) }
            fn at(&self, v: Self::Vector) -> Self {
                let [$($field),+] = v.0;
                Self { $($field: Dp::new($field)),+ }
            }
        }
    };
}

macro_rules! nested {
    ($last:tt) => { $last };
    ($first:tt, $($rest:tt),+) => { ($first, $crate::animation::nested!($($rest),+)) };
}

macro_rules! composite_fields {
    (
        $vis:vis struct $name:ident { $($field_vis:vis $field:ident: $part:ty),+ $(,)? }
        vectors $vectors:ident
    ) => {
        $crate::animation::composite_fields! { $vis struct $name { $($field_vis $field: $part),+ } }
        $vis struct $vectors<'a> {
            $($field_vis $field: &'a <$part as $crate::animation::Animatable>::Vector),+
        }
        impl<'a> $vectors<'a> {
            $vis fn of(vector: &'a <$name as $crate::animation::Animatable>::Vector) -> Self {
                let $crate::animation::nested!($($field),+) = vector;
                Self { $($field),+ }
            }
        }
    };
    ($vis:vis struct $name:ident { $($field_vis:vis $field:ident: $part:ty),+ $(,)? }) => {
        #[derive(Clone, PartialEq)]
        $vis struct $name { $($field_vis $field: $part),+ }
        impl $crate::animation::Composite for $name {
            type Parts = $crate::animation::nested!($($part),+);
            fn parts(&self) -> Self::Parts {
                $crate::animation::nested!($((self.$field.clone())),+)
            }
            fn with_parts(&self, parts: Self::Parts) -> Self {
                let $crate::animation::nested!($($field),+) = parts;
                Self { $($field),+ }
            }
        }
    };
}

pub(crate) use {composite_fields, nested};

value!(Point, 2, x, y);

value!(Size, 2, width, height);

value!(Rect, 4, x, y, width, height);

impl Rect {
    pub(crate) fn vector_position(vector: &Vector<4>) -> [f64; 2] {
        [vector.0[0], vector.0[1]]
    }

    pub(crate) fn vector_size(vector: &Vector<4>) -> [f64; 2] {
        [vector.0[2], vector.0[3]]
    }
}

value!(Insets, 4, top, right, bottom, left);

impl Composite for Corners {
    type Parts = [Dp; 4];

    fn parts(&self) -> [Dp; 4] {
        self.to_array()
    }

    fn with_parts(&self, [top_left, top_right, bottom_right, bottom_left]: [Dp; 4]) -> Self {
        Self::new(top_left, top_right, bottom_right, bottom_left)
    }
}

impl Composite for RoundedRect {
    type Parts = (Rect, Corners);

    fn parts(&self) -> (Rect, Corners) {
        (self.rect, self.corners)
    }

    fn with_parts(&self, (rect, corners): (Rect, Corners)) -> Self {
        Self::new(rect, corners)
    }
}

impl Composite for ScaleFactor {
    type Parts = f64;

    fn parts(&self) -> f64 {
        self.get().ln()
    }

    fn with_parts(&self, log: f64) -> Self {
        Self::saturating(log.exp()).unwrap_or(*self)
    }
}

impl Animatable for Affine {
    type Vector = Vector<6>;

    fn vector(&self) -> Self::Vector {
        let Self {
            xx,
            xy,
            yx,
            yy,
            translation,
        } = *self;
        Vector([xx, xy, yx, yy, translation.x.get(), translation.y.get()])
    }

    fn visible_step(pixel: Dp) -> f64 {
        f64::visible_step(pixel).min(Dp::visible_step(pixel))
    }

    fn at(&self, v: Self::Vector) -> Self {
        let [xx, xy, yx, yy, tx, ty] = v.0;
        Self {
            xx,
            xy,
            yx,
            yy,
            translation: Point::new(Dp::new(tx), Dp::new(ty)),
        }
    }
}

impl Animatable for f64 {
    type Vector = Vector<1>;

    fn vector(&self) -> Self::Vector {
        Vector([*self])
    }

    fn visible_step(_pixel: Dp) -> f64 {
        finest_step()
    }

    fn at(&self, v: Self::Vector) -> Self {
        v.0[0]
    }
}

impl Animatable for Dp {
    type Vector = Vector<1>;

    fn vector(&self) -> Self::Vector {
        self.get().vector()
    }

    fn visible_step(pixel: Dp) -> f64 {
        pixel.get() * finest_step()
    }

    fn at(&self, v: Self::Vector) -> Self {
        Dp::new(v.0[0])
    }
}

impl<T: Animatable, const N: usize> Animatable for [T; N] {
    type Vector = [T::Vector; N];

    fn vector(&self) -> Self::Vector {
        std::array::from_fn(|i| self[i].vector())
    }

    fn visible_step(pixel: Dp) -> f64 {
        T::visible_step(pixel)
    }

    fn at(&self, v: Self::Vector) -> Self {
        std::array::from_fn(|i| self[i].at(v[i].clone()))
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        let parts: [_; N] =
            std::array::from_fn(|i| self[i].retarget(&previous[i], current.map(|v| v[i].clone())));
        Trajectory {
            position: std::array::from_fn(|i| parts[i].position.clone()),
            velocity: std::array::from_fn(|i| parts[i].velocity.clone()),
            end: std::array::from_fn(|i| parts[i].end.clone()),
        }
    }

    fn spanning(&self, previous: &Self) -> Self {
        std::array::from_fn(|i| self[i].spanning(&previous[i]))
    }
}

impl<A: VectorSpace, B: VectorSpace> VectorSpace for (A, B) {
    fn zero() -> Self {
        (A::zero(), B::zero())
    }

    fn add(&self, rhs: &Self) -> Self {
        (self.0.add(&rhs.0), self.1.add(&rhs.1))
    }

    fn sub(&self, rhs: &Self) -> Self {
        (self.0.sub(&rhs.0), self.1.sub(&rhs.1))
    }

    fn scale(&self, factor: f64) -> Self {
        (self.0.scale(factor), self.1.scale(factor))
    }

    fn saturating_scale(&self, factor: f64) -> Self {
        (
            self.0.saturating_scale(factor),
            self.1.saturating_scale(factor),
        )
    }

    fn max_abs(&self) -> f64 {
        self.0.max_abs().max(self.1.max_abs())
    }

    fn finite(&self) -> bool {
        self.0.finite() && self.1.finite()
    }
}

impl<A: Animatable, B: Animatable> Animatable for (A, B) {
    type Vector = (A::Vector, B::Vector);

    fn vector(&self) -> Self::Vector {
        (self.0.vector(), self.1.vector())
    }

    fn visible_step(pixel: Dp) -> f64 {
        A::visible_step(pixel).min(B::visible_step(pixel))
    }

    fn at(&self, v: Self::Vector) -> Self {
        (self.0.at(v.0), self.1.at(v.1))
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        let a = self.0.retarget(&previous.0, current.map(|v| v.0.clone()));
        let b = self.1.retarget(&previous.1, current.map(|v| v.1.clone()));
        Trajectory {
            position: (a.position, b.position),
            velocity: (a.velocity, b.velocity),
            end: (a.end, b.end),
        }
    }

    fn spanning(&self, previous: &Self) -> Self {
        (self.0.spanning(&previous.0), self.1.spanning(&previous.1))
    }
}
