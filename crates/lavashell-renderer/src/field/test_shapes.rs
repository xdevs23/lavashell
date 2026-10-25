// SPDX-License-Identifier: GPL-3.0-only
use super::program::SMOOTH_MIN_DEPTH;
use super::smoothing;
use crate::test_error::TestError;
use lavashell_core::prelude::*;
use lavashell_core::shape::PointsError;

pub(crate) fn star(points: u16, inner: f64) -> Result<Shape, PointsError> {
    polygon((0..points).map(|i| {
        let angle = std::f64::consts::TAU * f64::from(i) / f64::from(points);
        let radius = if i % 2 == 0 { 50.0 } else { inner };
        Point::new(
            Dp::new(100.0 + radius * 1.6 * angle.cos()),
            Dp::new(60.0 + radius * angle.sin()),
        )
    }))
}

pub(crate) fn ellipse(points: u16, bounds: Rect) -> Result<Shape, PointsError> {
    polygon((0..points).map(|i| {
        let angle = std::f64::consts::TAU * f64::from(i) / f64::from(points);
        Point::new(
            bounds.x + bounds.width / 2.0 * (1.0 + angle.cos()),
            bounds.y + bounds.height / 2.0 * (1.0 + angle.sin()),
        )
    }))
}

fn polygon(points: impl Iterator<Item = Point>) -> Result<Shape, PointsError> {
    Ok(Shape::Contour(Polygon::new(points.collect::<Vec<_>>())?))
}

/// The union of `members`. Fails when `members` is empty.
pub(crate) fn union(
    members: impl IntoIterator<Item = Shape>,
    spacing: Dp,
) -> Result<Shape, TestError> {
    Shape::union(members, NonNegativeDp::saturating(spacing)?).ok_or(TestError::NoMembers)
}

pub(crate) trait Distance {
    fn distance(&self, p: Point) -> f64;
}

impl Distance for Shape {
    fn distance(&self, p: Point) -> f64 {
        match self {
            Shape::Rounded(rounded) => rounded.distance(p),
            Shape::Contour(polygon) => polygon.distance(p),
            Shape::Union(union) => {
                // The smooth minimum of infinity and the first distance is that distance, so the fold starts at
                // infinity.
                let fused = (union.members().iter())
                    .map(|shape| shape.distance(p))
                    .fold(f64::INFINITY, |a, b| smooth_min(a, b, smoothing(union)));
                let bounds = RoundedRect::new(self.bounds(), Corners::default());
                fused.max(bounds.distance(p))
            }
            Shape::Hollow(shape) => shape.outer().distance(p).max(-shape.hole().distance(p)),
        }
    }
}

impl Distance for RoundedRect {
    fn distance(&self, p: Point) -> f64 {
        let r = self.rect;
        let half = Point::new(r.width / 2.0, r.height / 2.0);
        let q = Point::new(p.x - r.x - half.x, p.y - r.y - half.y);
        let c = self.fitted_corners();
        let radius = match (q.x > 0.dp(), q.y > 0.dp()) {
            (false, false) => c.top_left,
            (true, false) => c.top_right,
            (true, true) => c.bottom_right,
            (false, true) => c.bottom_left,
        };
        let dx = q.x.abs() - half.x + radius;
        let dy = q.y.abs() - half.y + radius;
        (dx.max(dy).min(0.dp()) + dx.max(0.dp()).hypot(dy.max(0.dp())) - radius).get()
    }
}

impl Distance for Polygon {
    fn distance(&self, p: Point) -> f64 {
        let points = self.points();
        let mut nearest = f64::INFINITY;
        let mut inside = false;
        let numbers = |p: Point| (p.x.get(), p.y.get());
        let p = numbers(p);
        for (i, a) in points.iter().enumerate() {
            let (a, b) = (numbers(*a), numbers(points[(i + 1) % points.len()]));
            let edge = (b.0 - a.0, b.1 - a.1);
            let to = (p.0 - a.0, p.1 - a.1);
            let length = edge.0 * edge.0 + edge.1 * edge.1;
            let t = if length > 0.0 {
                ((to.0 * edge.0 + to.1 * edge.1) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            nearest = nearest.min((to.0 - edge.0 * t).hypot(to.1 - edge.1 * t));
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + to.1 * edge.0 / edge.1 {
                inside = !inside;
            }
        }
        if inside { -nearest } else { nearest }
    }
}

fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * SMOOTH_MIN_DEPTH
}

mod tests {
    use super::*;
    use crate::float_asserts::same_at;

    /// Asserts that the distance of `shape` at `p` is `expected` up to the spacing of `f64` at the largest coordinate
    /// of the shape and the point, the size of the numbers that the distance is computed from.
    fn assert_distance(shape: &Shape, p: Point, expected: f64) {
        let bounds = shape.bounds();
        let far = Point::new(bounds.x + bounds.width, bounds.y + bounds.height);
        let size = [bounds.x, bounds.y, far.x, far.y, p.x, p.y]
            .into_iter()
            .fold(0.0_f64, |size, coordinate| size.max(coordinate.get().abs()));
        let actual = shape.distance(p);
        assert!(same_at(actual, expected, size), "{actual} != {expected}");
    }

    fn island(x: f64, width: f64) -> Shape {
        RoundedRect::new(
            Rect::new(Dp::new(x), 0.dp(), Dp::new(width), 40.dp()),
            Corners::all(20.dp()),
        )
        .into()
    }

    #[test]
    fn rounded_distance_uses_the_radius_of_each_quadrant() {
        let shape: Shape = RoundedRect::new(
            Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp()),
            Corners::new(10.dp(), 0.dp(), 20.dp(), 5.dp()),
        )
        .into();
        assert_distance(&shape, Point::new(50.dp(), 20.dp()), -20.0);
        assert_distance(&shape, Point::new(105.dp(), 20.dp()), 5.0);
        assert_distance(&shape, Point::new(103.dp(), -4.dp()), 5.0);
        assert_distance(&shape, Point::new(4.dp(), 2.dp()), 0.0);
        assert_distance(&shape, Point::new(-2.dp(), -6.dp()), 10.0);
    }

    #[test]
    fn contour_distance_is_negative_inside_by_the_even_odd_rule() -> Result<(), TestError> {
        let triangle = Shape::Contour(Polygon::new([
            Point::new(0.dp(), 0.dp()),
            Point::new(40.dp(), 0.dp()),
            Point::new(0.dp(), 30.dp()),
        ])?);
        assert_distance(&triangle, Point::new(5.dp(), 5.dp()), -5.0);
        assert_distance(&triangle, Point::new(-3.dp(), 10.dp()), 3.0);
        assert_distance(&triangle, Point::new(40.dp(), 30.dp()), 24.0);
        Ok(())
    }

    #[test]
    fn union_members_fuse_once_their_gap_reaches_the_spacing() -> Result<(), TestError> {
        let islands = |gap: f64| union([island(0.0, 80.0), island(80.0 + gap, 80.0)], 24.dp());
        let center = |gap: f64| Point::new(Dp::new(80.0 + gap / 2.0), 20.dp());
        let gap_center = |gap: f64| Ok::<_, TestError>(islands(gap)?.distance(center(gap)));
        assert!(gap_center(40.0)? > 0.0);
        assert_distance(&islands(24.0)?, center(24.0), 0.0);
        assert!(gap_center(10.0)? < 0.0);
        assert!(gap_center(0.0)? < 0.0);
        Ok(())
    }

    #[test]
    fn union_keeps_aligned_edges_straight_and_far_members_unchanged() -> Result<(), TestError> {
        let pills = union([island(0.0, 80.0), island(90.0, 80.0)], 24.dp())?;
        // The neck between the two pills never bulges past their shared top or bottom edge.
        for y in [-0.5.dp(), 40.5.dp()] {
            assert!(pills.distance(Point::new(85.dp(), y)) > 0.0);
        }
        let far = union([island(0.0, 80.0), island(300.0, 80.0)], 24.dp())?;
        let left = island(0.0, 80.0);
        for p in [
            Point::new(-4.dp(), 20.dp()),
            Point::new(40.dp(), 20.dp()),
            Point::new(84.dp(), 20.dp()),
        ] {
            assert_distance(&far, p, left.distance(p));
        }
        Ok(())
    }
}
