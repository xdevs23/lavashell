// SPDX-License-Identifier: GPL-3.0-only
use super::{Animatable, Trajectory, Vector, VectorSpace};
use crate::geometry::{ContourPoint, Point};
use crate::numbers::count_f64;
use crate::shape::{PointsError, Polygon, Shape, check_points};
use crate::units::{Dp, NumberError};

/// The outline of a shape as at least two sample points, which a motion can animate into another
/// contour. The points are resampled when a target contour is created, never during a frame. A
/// motion moves each sample toward the sample with the same index in the target. A motion toward a
/// contour with another number of samples jumps to the target.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour<P = Point>(Vec<P>);

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
    /// The samples of the contour, in order.
    #[must_use]
    pub fn points(&self) -> &[P] {
        &self.0
    }

    /// A contour of `samples` points at equal arc lengths along the line through `points`. For a
    /// `closed` contour, the line also runs from the last point back to the first.
    ///
    /// # Errors
    ///
    /// Fails with fewer than two samples, fewer than two points, a point that is not finite, or
    /// points without length between them.
    pub fn resample(points: &[P], closed: bool, samples: usize) -> Result<Self, ContourError> {
        if samples < 2 {
            return Err(ContourError::TooFewSamples);
        }
        check_points(points, 2).map_err(ContourError::Points)?;
        let count = points.len() - usize::from(!closed);
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
        let spans =
            count_f64(if closed { samples } else { samples - 1 }).map_err(ContourError::Samples)?;
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

    /// This closed contour with the winding and the starting sample that make the sum of the
    /// squared distances to the samples of `reference` with the same index smallest. A motion
    /// between `reference` and the result then moves the samples as little as possible in total.
    ///
    /// # Errors
    ///
    /// Fails when `reference` has another number of samples.
    pub fn aligned_to(&self, reference: &Self) -> Result<Self, ContourError> {
        let n = self.0.len();
        if reference.0.len() != n {
            return Err(ContourError::SampleCounts);
        }
        let mut best = (f64::INFINITY, 0, false);
        for reverse in [false, true] {
            for offset in 0..n {
                let error = (0..n)
                    .map(|i| {
                        let j = if reverse {
                            (offset + n - i) % n
                        } else {
                            (offset + i) % n
                        };
                        let [ax, ay] = self.0[j].coordinates();
                        let [bx, by] = reference.0[i].coordinates();
                        let (dx, dy) = (ax - bx, ay - by);
                        dx.powi(2) + dy.powi(2)
                    })
                    .sum::<f64>();
                if error < best.0 {
                    best = (error, offset, reverse);
                }
            }
        }
        Ok(Self(
            (0..n)
                .map(|i| {
                    self.0[if best.2 {
                        (best.1 + n - i) % n
                    } else {
                        (best.1 + i) % n
                    }]
                })
                .collect(),
        ))
    }
}

impl Contour {
    /// A contour of `samples` points at equal arc lengths along the closed outline of `shape`, a
    /// rounded box or a contour. The outline follows rounded corners to within half of
    /// `device_pixel`.
    ///
    /// # Errors
    ///
    /// Fails when the shape has no single outline, or when its outline cannot be made or resampled.
    pub fn of(shape: &Shape, device_pixel: Dp, samples: usize) -> Result<Self, ContourError> {
        let outline = (shape.outline(device_pixel))
            .map_err(ContourError::Outline)?
            .ok_or(ContourError::Compound)?;
        Self::resample(&outline, true, samples)
    }
}
