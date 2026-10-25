// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::core::numbers::{count_f64, whole_count};
use lavashell::prelude::*;
use std::f64::consts::{FRAC_PI_2, TAU};

pub type Shape = Contour<PlainPoint>;

/// Design decisions of the demo, as fractions of the radius of the morph: the radius of the circle, the mean
/// radius of the star and how far the outline of the star extends beyond and inside that mean radius.
/// `STAR_POINTS` is the number of points of the star.
const CIRCLE: f64 = 0.91;
const STAR_MEAN: f64 = 0.74;
const STAR_DEPTH: f64 = 0.21;
const STAR_POINTS: f64 = 5.0;

/// An outline around the center, given as its radius at each angle.
#[derive(Clone, Copy)]
enum Outline {
    Circle,
    Star,
}

impl Outline {
    fn radius(self, angle: f64) -> f64 {
        match self {
            Self::Circle => CIRCLE,
            Self::Star => STAR_MEAN + STAR_DEPTH * (STAR_POINTS * (angle + FRAC_PI_2)).cos(),
        }
    }

    /// The smallest radius of curvature along the outline. The star curves most tightly where its radius
    /// has a maximum or a minimum: at the tips of its points and between them. There the first derivative of
    /// the radius is zero and the second derivative has its largest magnitude.
    fn tightest_curve(self) -> f64 {
        match self {
            Self::Circle => CIRCLE,
            Self::Star => {
                let turn = STAR_POINTS.powi(2) * STAR_DEPTH;
                let (outer, inner) = (STAR_MEAN + STAR_DEPTH, STAR_MEAN - STAR_DEPTH);
                (outer.powi(2) / (outer + turn)).min(inner.powi(2) / (turn - inner).abs())
            }
        }
    }

    /// An upper bound on the distance that a point of the outline moves per radian of angle.
    fn fastest_travel(self) -> f64 {
        match self {
            Self::Circle => CIRCLE,
            Self::Star => (STAR_MEAN + STAR_DEPTH).hypot(STAR_POINTS * STAR_DEPTH),
        }
    }

    /// The outline at `steps` equal angles, starting at the top.
    fn trace(self, steps: usize) -> Result<Vec<PlainPoint>, Error> {
        let step = TAU / count_f64(steps).map_err(Error::MorphSamples)?;
        (0..steps)
            .map(|index| {
                let angle = count_f64(index).map_err(Error::MorphSamples)? * step - FRAC_PI_2;
                let radius = self.radius(angle);
                Ok(PlainPoint::new(radius * angle.cos(), radius * angle.sin()))
            })
            .collect()
    }
}

/// The length around the closed polygon of `points`.
fn perimeter(points: &[PlainPoint]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| (b.x - a.x).hypot(b.y - a.y))
        .sum()
}

/// The circle and the star, sampled with the same number of points and aligned to each other. No edge
/// is further than `tolerance` from the outline that it follows, as a fraction of the radius of the morph.
///
/// Half of the tolerance is allowed for the polygon traced along each outline. The other half is
/// allowed for the edges between the samples taken along that polygon. An edge as long as `chord` is
/// at most half the tolerance away from a curve with the tightest radius of curvature.
pub fn outlines(tolerance: f64) -> Result<(Shape, Shape), Error> {
    let curve = Outline::Circle
        .tightest_curve()
        .min(Outline::Star.tightest_curve());
    let sagitta = (tolerance / 2.0).clamp(0.0, curve);
    let chord = 2.0 * (sagitta * (2.0 * curve - sagitta)).sqrt();
    let count = |length: f64| whole_count((length / chord).ceil()).map_err(Error::MorphSamples);
    let travel = Outline::Circle
        .fastest_travel()
        .max(Outline::Star.fastest_travel());
    let steps = count(TAU * travel)?;
    let (circle, star) = (Outline::Circle.trace(steps)?, Outline::Star.trace(steps)?);
    let samples = count(perimeter(&circle).max(perimeter(&star)))?;
    let circle = Shape::resample(&circle, true, samples).map_err(Error::Contour)?;
    let star = Shape::resample(&star, true, samples)
        .and_then(|star| star.aligned_to(&circle))
        .map_err(Error::Contour)?;
    Ok((circle, star))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The largest distance between an edge of `shape` and the circle of radius `CIRCLE`.
    fn departure_from_circle(shape: &Shape) -> f64 {
        let points = shape.points();
        let midpoints = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| f64::midpoint(a.x, b.x).hypot(f64::midpoint(a.y, b.y)));
        midpoints.map(|middle| CIRCLE - middle).fold(0.0, f64::max)
    }

    #[test]
    fn a_finer_tolerance_takes_more_samples_and_keeps_the_circle_within_it() -> Result<(), Error> {
        let (coarse, coarse_star) = outlines(0.01)?;
        let (fine, fine_star) = outlines(0.001)?;
        assert_eq!(coarse.points().len(), coarse_star.points().len());
        assert_eq!(fine.points().len(), fine_star.points().len());
        assert!(fine.points().len() > coarse.points().len());
        assert!(departure_from_circle(&coarse) <= 0.01);
        assert!(departure_from_circle(&fine) <= 0.001);
        Ok(())
    }

    #[test]
    fn a_tolerance_that_is_no_length_is_an_error() {
        assert!(matches!(outlines(0.0), Err(Error::MorphSamples(_))));
        assert!(matches!(outlines(f64::NAN), Err(Error::MorphSamples(_))));
    }
}
