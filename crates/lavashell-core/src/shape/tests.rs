// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::numbers::count_f64;
use crate::test_error::{Required, TestError, TestResult};
use crate::units::{Density, DpExt, NonNegativeDp, NumberError, PxExt};
use std::f64::consts::FRAC_PI_2;

fn union<const N: usize>(members: [Shape; N], spacing: Dp) -> Result<Shape, TestError> {
    Shape::union(members, NonNegativeDp::saturating(spacing)?).required("union")
}

fn island(x: Dp, width: Dp) -> Shape {
    RoundedRect::new(Rect::new(x, 0.dp(), width, 40.dp()), Corners::all(20.dp())).into()
}

fn triangle(x: Dp, y: Dp) -> Result<Shape, PointsError> {
    let polygon = Polygon::new([
        Point::new(x, y),
        Point::new(x + 4.dp(), y),
        Point::new(x, y + 4.dp()),
    ])?;
    Ok(Shape::Contour(polygon))
}

#[test]
fn radii_larger_than_half_the_shorter_side_are_drawn_as_a_stadium() {
    let shape = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp()),
        Corners::all(80.dp()),
    );
    assert_eq!(shape.fitted_corners(), Corners::all(20.dp()));
}

#[test]
fn outset_moves_every_side_and_radius_and_never_turns_negative() {
    let shape = RoundedRect::new(
        Rect::new(10.dp(), 10.dp(), 20.dp(), 8.dp()),
        Corners::new(4.dp(), 0.dp(), 2.dp(), 6.dp()),
    );
    let grown = shape.outset(3.dp());
    assert_eq!(grown.rect, Rect::new(7.dp(), 7.dp(), 26.dp(), 14.dp()));
    assert_eq!(grown.corners, Corners::new(7.dp(), 3.dp(), 5.dp(), 9.dp()));
    let shrunk = shape.outset(-5.dp());
    assert_eq!(shrunk.rect.height, 0.dp());
    assert_eq!(shrunk.corners, Corners::new(0.dp(), 0.dp(), 0.dp(), 1.dp()));
}

#[test]
fn rounded_outline_follows_the_boundary_clockwise() -> TestResult {
    let shape = RoundedRect::new(
        Rect::new(3.dp(), 5.dp(), 120.dp(), 48.dp()),
        Corners::new(24.dp(), 2.dp(), 0.dp(), 12.dp()),
    );
    let pixel = Density::ONE.to_dp(1.px());
    let outline = shape.outline(pixel)?;
    let r = shape.rect;
    for p in &outline {
        assert!(p.x >= r.x && p.x <= r.x + r.width, "{p:?}");
        assert!(p.y >= r.y && p.y <= r.y + r.height, "{p:?}");
    }
    let doubled_area: f64 = outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .map(|(a, b)| a.x.get() * b.y.get() - b.x.get() * a.y.get())
        .sum();
    // Positive in a y-down space means clockwise on screen.
    assert!(doubled_area > 0.0);
    let squares: f64 = shape
        .fitted_corners()
        .to_array()
        .iter()
        .map(|r| r.get() * r.get())
        .sum();
    let area = r.width.get() * r.height.get() - (1.0 - std::f64::consts::FRAC_PI_4) * squares;
    let missing = area - doubled_area / 2.0;
    // The polygon with its vertices on the corner arcs encloses less area than the rounded box. The difference is at
    // most the area of a strip half a pixel wide along each quarter arc.
    let radii: f64 = shape.fitted_corners().to_array().map(Dp::get).iter().sum();
    let bound = pixel.get() / 2.0 * FRAC_PI_2 * radii;
    assert!((0.0..=bound).contains(&missing), "{missing}");
    Ok(())
}

#[test]
fn a_corner_without_a_radius_is_one_point_of_the_outline() {
    let rect = Rect::new(3.dp(), 5.dp(), 120.dp(), 48.dp());
    let shape = RoundedRect::new(rect, Corners::all(0.dp()));
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    assert_eq!(
        shape.outline(Density::ONE.to_dp(1.px())),
        Ok(vec![
            Point::new(rect.x, rect.y),
            Point::new(right, rect.y),
            Point::new(right, bottom),
            Point::new(rect.x, bottom),
        ])
    );
}

#[test]
fn bounds_enclose_every_member() -> TestResult {
    let contour = triangle(0.dp(), 0.dp())?;
    assert_eq!(contour.bounds(), Rect::new(0.dp(), 0.dp(), 4.dp(), 4.dp()));
    let pills = union([island(0.dp(), 80.dp()), island(90.dp(), 80.dp())], 24.dp())?;
    assert_eq!(pills.bounds(), Rect::new(0.dp(), 0.dp(), 170.dp(), 40.dp()));
    Ok(())
}

#[test]
fn translated_shapes_move_every_member() -> TestResult {
    let shape = union([island(0.dp(), 80.dp()), triangle(0.dp(), 0.dp())?], 8.dp())?;
    let moved = union(
        [
            RoundedRect::new(
                Rect::new(10.dp(), -5.dp(), 80.dp(), 40.dp()),
                Corners::all(20.dp()),
            )
            .into(),
            triangle(10.dp(), -5.dp())?,
        ],
        8.dp(),
    )?;
    assert_eq!(shape.translated(Point::new(10.dp(), -5.dp())), moved);
    Ok(())
}

#[test]
fn shapes_refuse_short_or_infinite_polygons_and_empty_unions() {
    let corner = [Point::new(0.dp(), 0.dp()), Point::new(4.dp(), 0.dp())];
    assert_eq!(Polygon::new(corner), Err(PointsError::TooFew));
    assert_eq!(
        Polygon::new([corner[0], corner[1], Point::new(Dp::new(f64::NAN), 4.dp())]),
        Err(PointsError::NotFinite)
    );
    assert_eq!(Shape::union([], NonNegativeDp::ZERO), None);
}

#[test]
fn a_negative_spacing_counts_as_zero() -> TestResult {
    let Shape::Union(union) = union([island(0.dp(), 40.dp())], (-3).dp())? else {
        return Err(TestError::Absent("a union"));
    };
    assert_eq!(union.spacing(), Dp::ZERO);
    Ok(())
}

#[test]
fn a_rounded_rectangle_contains_points_inside_its_corners_as_drawn() {
    let pane = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 200.dp(), 100.dp()),
        Corners::all(28.dp()),
    );
    assert!(!pane.contains(Point::new(4.dp(), 4.dp())));
    assert!(pane.contains(Point::new(10.dp(), 10.dp())));
    assert!(pane.contains(Point::new(100.dp(), 50.dp())));
    assert!(pane.contains(Point::new(0.dp(), 50.dp())));
    assert!(!pane.contains(Point::new(196.dp(), 96.dp())));
    assert!(!pane.contains(Point::new(201.dp(), 50.dp())));
}

#[test]
fn a_rounded_rectangle_contains_points_inside_corners_fitted_to_its_sides() {
    let pill = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp()),
        Corners::all(100.dp()),
    );
    assert!(!pill.contains(Point::new(3.dp(), 3.dp())));
    assert!(pill.contains(Point::new(20.dp(), 3.dp())));
}

#[test]
fn a_pill_at_a_fractional_position_contains_points_inside_its_ends() {
    let pill = RoundedRect::new(
        Rect::new(0.1.dp(), 0.dp(), 2.dp(), 2.dp()),
        Corners::all(1.dp()),
    );
    assert!(pill.contains(Point::new(1.1.dp(), 0.5.dp())));
    assert!(!pill.contains(Point::new(0.15.dp(), 0.05.dp())));
}

fn card() -> RoundedRect {
    RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 200.dp(), 100.dp()),
        Corners::all(20.dp()),
    )
}

fn lens(rect: Rect) -> RoundedRect {
    RoundedRect::new(rect, Corners::all(Dp::INFINITY))
}

#[test]
fn a_shape_with_a_hole_has_the_bounds_of_its_card_and_no_single_outline() {
    let reaching = card().minus(lens(Rect::new(150.dp(), 30.dp(), 100.dp(), 40.dp())));
    let outside = card().minus(lens(Rect::new(300.dp(), 30.dp(), 80.dp(), 40.dp())));
    let covering = card().minus(RoundedRect::new(
        Rect::new(-10.dp(), -10.dp(), 220.dp(), 120.dp()),
        Corners::default(),
    ));
    let equal = card().minus(card());
    for shape in [reaching, outside, covering, equal] {
        assert!(matches!(shape, Shape::Hollow(_)), "{shape:?}");
        assert_eq!(shape.bounds(), card().rect);
        assert_eq!(shape.outline(Density::ONE.to_dp(1.px())), Ok(None));
        assert_eq!(*shape.without_holes(), Shape::Rounded(card()));
    }
}

#[test]
fn a_hole_without_area_leaves_the_card_as_it_is() {
    for (width, height) in [(0.dp(), 40.dp()), (80.dp(), 0.dp()), (-4.dp(), 40.dp())] {
        let hole = lens(Rect::new(60.dp(), 30.dp(), width, height));
        assert_eq!(card().minus(hole), Shape::Rounded(card()), "{hole:?}");
    }
}

#[test]
fn a_shape_with_a_hole_moves_its_card_and_hole_together() {
    let hole = lens(Rect::new(60.dp(), 30.dp(), 80.dp(), 40.dp()));
    let offset = Point::new(10.dp(), -5.dp());
    assert_eq!(
        card().minus(hole).translated(offset),
        card().translated(offset).minus(hole.translated(offset))
    );
}

#[test]
fn a_union_without_holes_fills_the_holes_of_its_members() -> TestResult {
    let hollow = card().minus(lens(Rect::new(60.dp(), 30.dp(), 80.dp(), 40.dp())));
    let pills = union([island(0.dp(), 80.dp()), island(90.dp(), 80.dp())], 8.dp())?;
    assert!(matches!(pills.without_holes(), Cow::Borrowed(_)));
    let mixed = union([hollow, island(300.dp(), 80.dp())], 8.dp())?;
    assert_eq!(
        *mixed.without_holes(),
        union([card().into(), island(300.dp(), 80.dp())], 8.dp())?
    );
    Ok(())
}

#[test]
fn infinite_corners_draw_a_pill() {
    let pill = RoundedRect::new(
        Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp()),
        Corners::all(Dp::INFINITY),
    );
    assert_eq!(pill.fitted_corners(), Corners::all(20.dp()));
    assert!(!pill.contains(Point::new(3.dp(), 3.dp())));
    assert!(pill.contains(Point::new(100.dp(), 3.dp())));
}

#[test]
fn negative_infinite_corners_and_corners_that_are_no_number_draw_square_corners() {
    let rect = Rect::new(0.dp(), 0.dp(), 200.dp(), 40.dp());
    for radius in [-Dp::INFINITY, Dp::new(f64::NAN)] {
        let square = RoundedRect::new(rect, Corners::all(radius));
        assert_eq!(square.fitted_corners(), Corners::all(Dp::ZERO));
    }
}

#[test]
fn the_fewest_chords_of_a_quarter_turn_lie_within_the_tolerance_inside_the_arc() -> TestResult {
    // When a quarter turn is cut into `chords` chords, the middle of each chord lies `2 r sin(angle / 4)^2` inside the
    // arc, where `angle` is the angle covered by one chord.
    let inside = |radius: f64, chords: usize| -> Result<f64, NumberError> {
        Ok(2.0 * radius * (FRAC_PI_2 / count_f64(chords)? / 4.0).sin().powi(2))
    };
    let tolerance = 0.5;
    for radius in [0.25, 3.0, 24.0, 1000.0] {
        let chords = outline::chords_per_quarter_turn(radius, tolerance)?;
        assert!(inside(radius, chords)? <= tolerance, "{radius}");
        assert!(
            chords == 1 || inside(radius, chords - 1)? > tolerance,
            "{radius}"
        );
    }
    Ok(())
}
