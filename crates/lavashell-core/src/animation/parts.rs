// SPDX-License-Identifier: GPL-3.0-only
use super::{Animatable, Trajectory, VectorSpace};
use crate::units::Dp;

impl<V: VectorSpace> VectorSpace for Vec<V> {
    fn zero() -> Self {
        Vec::new()
    }

    fn add(&self, rhs: &Self) -> Self {
        let len = self.len().max(rhs.len());
        (0..len)
            .map(|i| entry(self, i).add(&entry(rhs, i)))
            .collect()
    }

    fn sub(&self, rhs: &Self) -> Self {
        let len = self.len().max(rhs.len());
        (0..len)
            .map(|i| entry(self, i).sub(&entry(rhs, i)))
            .collect()
    }

    fn scale(&self, factor: f64) -> Self {
        self.iter().map(|v| v.scale(factor)).collect()
    }

    fn saturating_scale(&self, factor: f64) -> Self {
        self.iter().map(|v| v.saturating_scale(factor)).collect()
    }

    fn max_abs(&self) -> f64 {
        self.iter().fold(0.0_f64, |a, v| a.max(v.max_abs()))
    }

    fn finite(&self) -> bool {
        self.iter().all(VectorSpace::finite)
    }
}

pub(crate) fn entry<V: VectorSpace>(vectors: &[V], i: usize) -> V {
    vectors.get(i).cloned().unwrap_or_else(V::zero)
}

pub trait Fading: Animatable {
    fn faded(&self) -> Self;
}

fn retarget_each<T: Fading>(
    parts: &[T],
    previous: &[T],
    current: &Trajectory<Vec<T::Vector>>,
) -> Trajectory<Vec<T::Vector>> {
    (parts.iter().enumerate())
        .map(|(i, part)| match previous.get(i) {
            Some(previous) => part.retarget(previous, current.map(|v| entry(v, i))),
            None => Trajectory {
                end: part.vector(),
                ..Trajectory::at_rest(part.faded().vector())
            },
        })
        .collect()
}

impl<T: Fading> Animatable for Vec<T> {
    type Vector = Vec<T::Vector>;

    fn vector(&self) -> Self::Vector {
        self.iter().map(Animatable::vector).collect()
    }

    fn visible_step(pixel: Dp) -> f64 {
        T::visible_step(pixel)
    }

    fn at(&self, vector: Self::Vector) -> Self {
        self.iter()
            .enumerate()
            .map(|(i, part)| part.at(entry(&vector, i)))
            .collect()
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        retarget_each(self, previous, &current)
    }

    fn spanning(&self, previous: &Self) -> Self {
        let paired = self
            .iter()
            .zip(previous)
            .map(|(part, previous)| part.spanning(previous));
        let extra = previous.iter().skip(self.len()).map(Fading::faded);
        paired
            .chain(self.iter().skip(previous.len()).cloned())
            .chain(extra)
            .collect()
    }
}

impl<T: Fading> Animatable for Option<T> {
    type Vector = Vec<T::Vector>;

    fn vector(&self) -> Self::Vector {
        self.iter().map(Animatable::vector).collect()
    }

    fn visible_step(pixel: Dp) -> f64 {
        T::visible_step(pixel)
    }

    fn at(&self, vector: Self::Vector) -> Self {
        self.as_ref().map(|part| part.at(entry(&vector, 0)))
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        retarget_each(self.as_slice(), previous.as_slice(), &current)
    }

    fn spanning(&self, previous: &Self) -> Self {
        match (self, previous) {
            (Some(part), Some(previous)) => Some(part.spanning(previous)),
            (None, Some(previous)) => Some(previous.faded()),
            (part, None) => part.clone(),
        }
    }
}
