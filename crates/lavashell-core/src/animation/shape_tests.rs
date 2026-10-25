// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::shape::PointsError;
use crate::{
    geometry::*,
    shape::{Corners, RoundedRect, Shape},
    test_error::{Required, TestResult},
    units::{Density, Dp, DpExt, NonNegativeDp, PxExt},
};
use std::time::Duration;

#[test]
fn contour_arc_length_sampling_and_alignment_preserve_shape_and_winding() -> TestResult {
    let square = [
        Point::new(0.dp(), 0.dp()),
        Point::new(10.dp(), 0.dp()),
        Point::new(10.dp(), 10.dp()),
        Point::new(0.dp(), 10.dp()),
    ];
    let reference = Contour::resample(&square, true, 8)?;
    let reversed = Contour::resample(&[square[2], square[1], square[0], square[3]], true, 8)?;
    assert_eq!(reversed.aligned_to(&reference)?, reference);
    assert_eq!(
        Contour::resample(&square, true, 6)?.aligned_to(&reference),
        Err(ContourError::SampleCounts)
    );
    let open = Contour::resample(&square[..2], false, 5)?;
    assert_eq!(open.points().len(), 5);
    assert_eq!(open.points()[0], square[0]);
    assert_eq!(open.points()[4], square[1]);
    assert_eq!(open.points()[2], Point::new(5.dp(), 0.dp()));
    Ok(())
}

#[test]
fn a_motion_toward_a_contour_of_another_sample_count_rests_on_it() -> TestResult {
    let line = [Point::new(0.dp(), 0.dp()), Point::new(4.dp(), 0.dp())];
    let previous = Contour::resample(&line, false, 3)?;
    let target = Contour::resample(&line, false, 5)?;
    let moving = Trajectory {
        position: previous.vector(),
        velocity: previous.vector(),
        end: previous.vector(),
    };
    let retargeted = target.retarget(&previous, moving);
    assert_eq!(retargeted.position, target.vector());
    assert_eq!(retargeted.end, target.vector());
    assert_eq!(retargeted.velocity, Vec::<Vector<2>>::zero());
    Ok(())
}

#[test]
fn a_contour_of_plain_number_pairs_samples_like_a_contour_in_dp() -> TestResult {
    let square =
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| PlainPoint::new(x, y));
    let plain = Contour::resample(&square, true, 8)?;
    assert_eq!(plain.points()[1], PlainPoint::new(0.5, 0.0));
    let in_dp = |p: &PlainPoint| Point::new(Dp::new(p.x), Dp::new(p.y));
    let lengths = Contour::resample(&square.each_ref().map(in_dp), true, 8)?;
    assert!(
        plain
            .points()
            .iter()
            .map(in_dp)
            .eq(lengths.points().iter().copied())
    );
    Ok(())
}

#[test]
fn contour_sampling_reports_each_cause_of_bad_points() {
    let resample = |points: &[Point]| Contour::resample(points, false, 4);
    assert_eq!(
        resample(&[Point::new(1.dp(), 2.dp())]),
        Err(ContourError::Points(PointsError::TooFew))
    );
    let not_finite = [
        Point::new(0.dp(), 0.dp()),
        Point::new(Dp::new(f64::NAN), 1.dp()),
    ];
    assert_eq!(
        resample(&not_finite),
        Err(ContourError::Points(PointsError::NotFinite))
    );
    let coincident = [Point::new(3.dp(), 3.dp()), Point::new(3.dp(), 3.dp())];
    assert_eq!(resample(&coincident), Err(ContourError::ZeroLength));
    let line = [Point::new(0.dp(), 0.dp()), Point::new(1.dp(), 0.dp())];
    assert_eq!(
        Contour::resample(&line, false, 1),
        Err(ContourError::TooFewSamples)
    );
}

#[test]
fn any_outline_samples_into_a_contour_and_a_union_or_a_shape_with_a_hole_has_none() -> TestResult {
    let card = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 60.dp(), 40.dp()),
        Corners::all(12.dp()),
    );
    let shape: Shape = card.into();
    let pixel = Density::ONE.to_dp(1.px());
    let outline = shape.outline(pixel)?.required("outline")?;
    // A sample farther from the outline than half of the visible step would be visible on some output.
    let unseen = Dp::visible_step(pixel) / 2.0;
    let distance_to_edge = |p: Point, a: Point, b: Point| {
        let [(px, py), (ax, ay), (bx, by)] = [p, a, b].map(|q| (q.x.get(), q.y.get()));
        let (edge_x, edge_y) = (bx - ax, by - ay);
        let along = ((px - ax) * edge_x + (py - ay) * edge_y) / edge_x.hypot(edge_y).powi(2);
        let nearest = along.clamp(0.0, 1.0);
        (px - ax - nearest * edge_x).hypot(py - ay - nearest * edge_y)
    };
    let on_outline = |p: Point| {
        let mut edges = outline.iter().zip(outline.iter().cycle().skip(1));
        edges.any(|(a, b)| distance_to_edge(p, *a, *b) < unseen)
    };
    for &p in Contour::of(&shape, pixel, 64)?.points() {
        assert!(on_outline(p), "{p:?}");
    }
    let spacing = NonNegativeDp::try_new(4.dp())?;
    let joined = Shape::union([shape], spacing).required("union of shapes")?;
    assert_eq!(Contour::of(&joined, pixel, 8), Err(ContourError::Compound));
    let lens = RoundedRect::new(
        Rect::new(20.dp(), 10.dp(), 20.dp(), 20.dp()),
        Corners::all(10.dp()),
    );
    assert_eq!(
        Contour::of(&card.minus(lens), pixel, 8),
        Err(ContourError::Compound)
    );
    Ok(())
}

#[test]
fn rounded_rects_animate_their_bounds_and_each_corner_together() -> TestResult {
    let from = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 40.dp(), 40.dp()),
        Corners::all(20.dp()),
    );
    let to = RoundedRect::new(
        Rect::new(100.dp(), 0.dp(), 120.dp(), 40.dp()),
        Corners::new(20.dp(), 0.dp(), 0.dp(), 20.dp()),
    );
    let motion = Motion::default();
    let animation = motion.animation(from)?;
    animation.animate_to(to, Tween::new(Duration::from_millis(1000)))?;
    motion.advance(Duration::from_millis(500));
    assert_eq!(
        animation.get(),
        RoundedRect::new(
            Rect::new(50.dp(), 0.dp(), 80.dp(), 40.dp()),
            Corners::new(20.dp(), 10.dp(), 10.dp(), 20.dp())
        )
    );
    Ok(())
}
