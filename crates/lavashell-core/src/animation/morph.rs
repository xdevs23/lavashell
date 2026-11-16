// SPDX-License-Identifier: GPL-3.0-only
use super::{Animatable, Trajectory, Vector, VectorSpace};
use crate::geometry::{ContourPoint, Point};
use crate::numbers::count_f64;
use crate::shape::{PointsError, Polygon, Shape, check_points};
use crate::units::{Dp, NumberError};

#[derive(Clone, Debug, PartialEq)]
pub struct Contour<P = Point>(Vec<P>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closure {
    Open,
    Closed,
}

impl Closure {
    fn gaps_between(self, count: usize) -> usize {
        match self {
            Self::Open => count - 1,
            Self::Closed => count,
        }
    }
}

impl<P: ContourPoint> Animatable for Contour<P> {
    type Vector = Vec<Vector<2>>;

    fn vector(&self) -> Self::Vector {
        self.0.iter().map(|p| Vector(p.coordinates())).collect()
    }

    fn visible_step(pixel: Dp) -> f64 {
        P::visible_step(pixel)
    }

    fn at(&self, v: Self::Vector) -> Self {
        let sample = |index: usize| v.get(index).copied().unwrap_or_else(Vector::zero);
        Self(
            (0..self.0.len())
                .map(|index| P::from_coordinates(sample(index).0))
                .collect(),
        )
    }

    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        if previous.0.len() == self.0.len() {
            Trajectory {
                end: self.vector(),
                ..current
            }
        } else {
            Trajectory::at_rest(self.vector())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContourError {
    #[error("a contour needs at least two finite points")]
    Points(#[source] PointsError),
    #[error("the points of a contour have no length between them")]
    ZeroLength,
    #[error("a union or a shape with a hole has no single outline")]
    Compound,
    #[error("a contour needs at least two samples")]
    TooFewSamples,
    #[error("contours of different sample counts cannot be aligned")]
    SampleCounts,
    #[error("the samples of a contour are too many to count exactly")]
    Samples(#[source] NumberError),
    #[error("the outline of the shape could not be made")]
    Outline(#[source] NumberError),
}

impl TryFrom<Contour> for Shape {
    type Error = PointsError;

    fn try_from(contour: Contour) -> Result<Self, PointsError> {
        Polygon::new(contour.0).map(Self::Contour)
    }
}

impl<P: ContourPoint> Contour<P> {
    #[must_use]
    pub fn points(&self) -> &[P] {
        &self.0
    }

    pub fn resample(points: &[P], closure: Closure, samples: usize) -> Result<Self, ContourError> {
        if samples < 2 {
            return Err(ContourError::TooFewSamples);
        }
        check_points(points, 2).map_err(ContourError::Points)?;
        let count = closure.gaps_between(points.len());
        let mut distances = Vec::with_capacity(count + 1);
        distances.push(0.0_f64);
        for i in 0..count {
            let [ax, ay] = points[i].coordinates();
            let [bx, by] = points[(i + 1) % points.len()].coordinates();
            let (dx, dy) = (bx - ax, by - ay);
            distances.push(distances[i] + dx.hypot(dy));
        }
        let total = distances[count];
        if total == 0.0 {
            return Err(ContourError::ZeroLength);
        }
        let spans = count_f64(closure.gaps_between(samples)).map_err(ContourError::Samples)?;
        let lengths = (0..samples)
            .map(|index| count_f64(index).map(|index| total * index / spans))
            .collect::<Result<Vec<_>, _>>()
            .map_err(ContourError::Samples)?;
        let mut segment = 0;
        let resampled = lengths.into_iter().map(|at| {
            while segment + 1 < count && distances[segment + 1] < at {
                segment += 1;
            }
            let span = distances[segment + 1] - distances[segment];
            let t = if span > 0.0 {
                (at - distances[segment]) / span
            } else {
                0.0
            };
            let [ax, ay] = points[segment].coordinates();
            let [bx, by] = points[(segment + 1) % points.len()].coordinates();
            P::from_coordinates([ax + (bx - ax) * t, ay + (by - ay) * t])
        });
        Ok(Self(resampled.collect()))
    }

    pub fn aligned_to(&self, reference: &Self) -> Result<Self, ContourError> {
        let n = self.0.len();
        if reference.0.len() != n {
            return Err(ContourError::SampleCounts);
        }
        let error = |alignment: &Alignment| -> f64 {
            (0..n)
                .map(|i| {
                    let [ax, ay] = self.0[alignment.index(i, n)].coordinates();
                    let [bx, by] = reference.0[i].coordinates();
                    (ax - bx).powi(2) + (ay - by).powi(2)
                })
                .sum()
        };
        let best = [Winding::Forward, Winding::Reverse]
            .into_iter()
            .flat_map(|winding| (0..n).map(move |offset| Alignment { offset, winding }))
            .map(|alignment| (error(&alignment), alignment))
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, alignment)| alignment);
        let points = best
            .into_iter()
            .flat_map(|best| (0..n).map(move |i| best.index(i, n)));
        Ok(Self(points.map(|index| self.0[index]).collect()))
    }
}

#[derive(Clone, Copy)]
enum Winding {
    Forward,
    Reverse,
}

#[derive(Clone, Copy)]
struct Alignment {
    offset: usize,
    winding: Winding,
}

impl Alignment {
    fn index(&self, i: usize, n: usize) -> usize {
        match self.winding {
            Winding::Forward => (self.offset + i) % n,
            Winding::Reverse => (self.offset + n - i) % n,
        }
    }
}

impl Contour {
    pub fn of(shape: &Shape, device_pixel: Dp, samples: usize) -> Result<Self, ContourError> {
        let outline = (shape.outline(device_pixel))
            .map_err(ContourError::Outline)?
            .ok_or(ContourError::Compound)?;
        Self::resample(&outline, Closure::Closed, samples)
    }
}
