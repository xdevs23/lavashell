// SPDX-License-Identifier: GPL-3.0-only
use super::super::{Corners, Hollow, Polygon, RoundedRect, Shape, Union, UnionMember};
use crate::geometry::{Point, Rect};
use crate::test_error::{Required, TestError, TestResult};
use crate::units::{Dp, DpExt, NonNegativeDp};

fn square(x: Dp, size: Dp) -> RoundedRect {
    RoundedRect::new(Rect::new(x, x, size, size), Corners::default())
}

#[test]
fn a_union_reads_back_as_written() -> TestResult {
    let Shape::Hollow(hollow) = square(0.dp(), 30.dp()).minus(square(10.dp(), 10.dp())) else {
        return Err(TestError::Absent("a hollow square"));
    };
    let triangle = Polygon::new([
        Point::new(0.dp(), 0.dp()),
        Point::new(9.dp(), 0.dp()),
        Point::new(0.dp(), 9.dp()),
    ])?;
    let pill = RoundedRect::new(
        Rect::new(40.dp(), 0.dp(), 40.dp(), 20.dp()),
        Corners::all(10.dp()),
    );
    let members = [
        pill.into(),
        UnionMember::Contour(triangle),
        UnionMember::Hollow(hollow),
    ];
    let spacing = NonNegativeDp::try_new(6.dp())?;
    let union = Shape::union(members, spacing).required("a union")?;
    let bytes = postcard::to_allocvec(&union)?;
    let read = postcard::from_bytes::<Shape>(&bytes)?;
    assert_eq!(read.bounds(), union.bounds());
    assert_eq!(read, union);
    Ok(())
}

#[test]
fn a_union_without_members_is_not_read() -> TestResult {
    let empty = (Vec::<UnionMember>::new(), NonNegativeDp::ZERO);
    let bytes = postcard::to_allocvec(&empty)?;
    assert!(postcard::from_bytes::<Union>(&bytes).is_err());
    Ok(())
}

#[test]
fn a_polygon_with_two_points_is_not_read() -> TestResult {
    let line = [Point::new(0.dp(), 0.dp()), Point::new(5.dp(), 5.dp())];
    let bytes = postcard::to_allocvec(&line.to_vec())?;
    assert!(postcard::from_bytes::<Polygon>(&bytes).is_err());
    Ok(())
}

#[test]
fn a_hole_without_area_is_not_read() -> TestResult {
    let flat = RoundedRect::new(
        Rect::new(10.dp(), 10.dp(), 0.dp(), 10.dp()),
        Corners::default(),
    );
    let hollow = Shape::Hollow(Hollow {
        outer: square(0.dp(), 30.dp()),
        hole: flat,
    });
    let bytes = postcard::to_allocvec(&hollow)?;
    assert!(postcard::from_bytes::<Shape>(&bytes).is_err());
    Ok(())
}
