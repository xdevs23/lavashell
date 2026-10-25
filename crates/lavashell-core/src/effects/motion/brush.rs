// SPDX-License-Identifier: GPL-3.0-only
//! The animation of a brush. A solid brush animates as a gradient whose stops all have its color. When two
//! gradients have different numbers of stops, stops of the gradient with fewer stops are repeated to match the
//! other gradient. A change to another gradient kind, together with its coordinates, applies at the start of the
//! motion.

use super::{Interpolated, transparent};
use crate::animation::{
    Animatable, Fading, Trajectory, Vector, VectorSpace, composite_fields, entry,
};
use crate::color::ColorIn;
use crate::geometry::Color;
use crate::gradient::{ColorStop, Gradient, GradientKind};
use crate::scene::Brush;
use crate::units::Dp;
use std::mem::discriminant;

composite_fields! {
    pub struct Stop {
        position: f64,
        color: ColorIn,
    }
}

type StopVector = <Stop as Animatable>::Vector;

/// The coordinates of a gradient kind, in the order of `GradientKind::coordinates`.
type Coordinates = Vec<Vector<1>>;

/// The index of the stop among `count` stops that matches stop `index` among `target_count`
/// stops. The stops at both ends match each other. Every other stop matches the stop nearest to the same relative
/// position.
fn matching_stop(index: usize, count: usize, target_count: usize) -> usize {
    if count <= 1 || target_count <= 1 {
        return 0;
    }
    let target_steps = target_count - 1;
    (2 * index * (count - 1) + target_steps) / (2 * target_steps)
}

fn coordinates(kind: &GradientKind) -> Coordinates {
    let numbers = kind.coordinates().into_iter();
    numbers.map(|number| Vector([number])).collect()
}

/// `kind` moved to `coordinates`. When `coordinates` has another count of numbers than `kind`, because the
/// coordinates belong to another gradient kind, `kind` is returned unchanged.
fn kind_at(kind: &GradientKind, coordinates: &Coordinates) -> GradientKind {
    let numbers = coordinates.iter().map(|Vector([number])| *number);
    kind.with_coordinates(numbers).unwrap_or(*kind)
}

impl Interpolated<Brush> {
    fn stop(&self, position: f64, color: Color) -> Stop {
        Stop {
            position,
            color: self.in_space(color),
        }
    }

    fn stops(&self) -> Vec<Stop> {
        match &self.value {
            Brush::Solid(color) => vec![self.stop(0.0, *color)],
            Brush::Gradient(gradient) => (gradient.stops.iter())
                .map(|s| self.stop(s.position, s.color))
                .collect(),
        }
    }

    fn gradient(&self, gradient: &Gradient, stops: impl Iterator<Item = ColorStop>) -> Self {
        self.with_value(Brush::Gradient(Gradient {
            stops: stops.collect(),
            ..gradient.clone()
        }))
    }
}

impl Animatable for Interpolated<Brush> {
    type Vector = (Vec<StopVector>, Coordinates);

    fn vector(&self) -> Self::Vector {
        let geometry = match &self.value {
            Brush::Solid(_) => Coordinates::zero(),
            Brush::Gradient(gradient) => coordinates(&gradient.kind),
        };
        (self.stops().iter().map(Stop::vector).collect(), geometry)
    }

    /// The smallest of the steps for a stop, a length and a plain number. The coordinates of a gradient are
    /// lengths, except for the angles of a sweep gradient, which have no unit.
    fn visible_step(pixel: Dp) -> f64 {
        Stop::visible_step(pixel)
            .min(Dp::visible_step(pixel))
            .min(f64::visible_step(pixel))
    }

    fn at(&self, (stop_vectors, geometry): Self::Vector) -> Self {
        match &self.value {
            Brush::Solid(color) => {
                let stop = self.stop(0.0, *color).at(entry(&stop_vectors, 0));
                self.with_value(Brush::Solid(stop.color.color))
            }
            Brush::Gradient(gradient) => {
                let stops = (self.stops().into_iter().enumerate())
                    .map(|(i, stop)| stop.at(entry(&stop_vectors, i)))
                    .map(|stop| ColorStop::new(stop.position, stop.color.color));
                let kind = kind_at(&gradient.kind, &geometry);
                self.gradient(
                    &Gradient {
                        kind,
                        ..gradient.clone()
                    },
                    stops,
                )
            }
        }
    }

    /// Each stop moves from the matching stop of `previous`. When `previous` is a solid brush, each stop starts at
    /// rest at its final position and only its color moves. The coordinates move only between two gradients of the
    /// same kind.
    fn retarget(
        &self,
        previous: &Self,
        current: Trajectory<Self::Vector>,
    ) -> Trajectory<Self::Vector> {
        let (stops, before) = (self.stops(), previous.stops());
        let from_solid = matches!(previous.value, Brush::Solid(_));
        let each: Trajectory<Vec<StopVector>> = (stops.iter().enumerate())
            .map(|(i, stop)| {
                let k = matching_stop(i, before.len(), stops.len());
                let Some(previous) = before.get(k) else {
                    return Trajectory::at_rest(stop.vector());
                };
                let mut trajectory = current.map(|(stop_vectors, _)| entry(stop_vectors, k));
                if from_solid {
                    let (position, _) = &mut trajectory.position;
                    *position = stop.position.vector();
                    let (velocity, _) = &mut trajectory.velocity;
                    *velocity = Vector::zero();
                }
                stop.retarget(previous, trajectory)
            })
            .collect();
        let (_, end) = self.vector();
        let geometry = match (&self.value, &previous.value) {
            (Brush::Gradient(a), Brush::Gradient(b))
                if discriminant(&a.kind) == discriminant(&b.kind) =>
            {
                Trajectory {
                    end,
                    ..current.map(|(_, geometry)| geometry.clone())
                }
            }
            _ => Trajectory::at_rest(end),
        };
        Trajectory {
            position: (each.position, geometry.position),
            velocity: (each.velocity, geometry.velocity),
            end: (each.end, geometry.end),
        }
    }

    fn spanning(&self, previous: &Self) -> Self {
        match (&self.value, &previous.value) {
            (Brush::Solid(color), Brush::Gradient(other)) if !other.stops.is_empty() => {
                let stops = other
                    .stops
                    .iter()
                    .map(|s| ColorStop::new(s.position, *color));
                self.gradient(other, stops)
            }
            (Brush::Gradient(own), Brush::Gradient(other))
                if !own.stops.is_empty() && other.stops.len() > own.stops.len() =>
            {
                let (count, target_count) = (own.stops.len(), other.stops.len());
                let stops =
                    (0..target_count).map(|i| own.stops[matching_stop(i, count, target_count)]);
                self.gradient(own, stops)
            }
            _ => self.clone(),
        }
    }
}

impl Fading for Interpolated<Brush> {
    fn faded(&self) -> Self {
        match &self.value {
            Brush::Solid(color) => self.with_value(Brush::Solid(transparent(*color))),
            Brush::Gradient(gradient) => {
                let stops = (gradient.stops.iter())
                    .map(|stop| ColorStop::new(stop.position, transparent(stop.color)));
                self.gradient(gradient, stops)
            }
        }
    }
}
