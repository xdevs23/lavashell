// SPDX-License-Identifier: GPL-3.0-only
use super::{Check, Samples, TestError, TestResult};
use lavashell_core::geometry::{Point, Rect};
use lavashell_core::shape::{Corners, Hollow, Polygon, RoundedRect, Shape, UnionMember};
use lavashell_core::units::{DpExt, NonNegativeDp};

pub(super) fn rect() -> Rect {
    Rect::new(16.dp(), 24.dp(), 240.dp(), 164.dp())
}

pub(super) fn rounded() -> RoundedRect {
    RoundedRect::new(rect(), Corners::new(2.dp(), 4.dp(), 6.dp(), 8.dp()))
}

fn contour() -> Result<Polygon, TestError> {
    Ok(Polygon::new([
        Point::new(0.dp(), 0.dp()),
        Point::new(30.dp(), 4.dp()),
        Point::new(12.dp(), 20.dp()),
    ])?)
}

fn hollow() -> Result<Hollow, TestError> {
    let hole = RoundedRect::new(
        Rect::new(40.dp(), 48.dp(), 20.dp(), 10.dp()),
        Corners::all(5.dp()),
    );
    match rounded().minus(hole) {
        Shape::Hollow(hollow) => Ok(hollow),
        Shape::Rounded(_) | Shape::Contour(_) | Shape::Union(_) => {
            Err(TestError::Absent("a hole inside the rectangle"))
        }
    }
}

fn shapes() -> Result<Samples<Shape>, TestError> {
    let members = [
        UnionMember::Rounded(rounded()),
        UnionMember::Contour(contour()?),
    ];
    let fused = Shape::union(members, NonNegativeDp::try_new(4.dp())?)
        .ok_or(TestError::Absent("a union with members"))?;
    Ok(every_variant! {
        Shape::Rounded(_) => Shape::Rounded(rounded()), [
            0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0,
            0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64,
        ];
        Shape::Contour(_) => Shape::Contour(contour()?), [
            1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 64, 0, 0, 0,
            0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 40, 64, 0, 0, 0, 0, 0, 0, 52, 64,
        ];
        Shape::Union(_) => fused, [
            2, 2, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64,
            0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0,
            0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 40, 64,
            0, 0, 0, 0, 0, 0, 52, 64, 0, 0, 0, 0, 0, 0, 16, 64,
        ];
        Shape::Hollow(_) => Shape::Hollow(hollow()?), [
            3, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0,
            0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 68, 64, 0, 0, 0, 0, 0, 0, 72, 64,
            0, 0, 0, 0, 0, 0, 52, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0,
            0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 20, 64,
        ];
    })
}

fn union_members() -> Result<Samples<UnionMember>, TestError> {
    Ok(every_variant! {
        UnionMember::Rounded(_) => UnionMember::Rounded(rounded()), [
            0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0,
            0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64,
        ];
        UnionMember::Contour(_) => UnionMember::Contour(contour()?), [
            1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 64, 0, 0, 0,
            0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 40, 64, 0, 0, 0, 0, 0, 0, 52, 64,
        ];
        UnionMember::Hollow(_) => UnionMember::Hollow(hollow()?), [
            2, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0,
            0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0,
            0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 68, 64, 0, 0, 0, 0, 0, 0, 72, 64,
            0, 0, 0, 0, 0, 0, 52, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0,
            0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 20, 64, 0, 0, 0, 0, 0, 0, 20, 64,
        ];
    })
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&shapes()?)?;
    check.samples(&union_members()?)
}
