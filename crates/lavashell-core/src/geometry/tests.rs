// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::DpExt;

#[test]
fn only_an_offered_finite_length_allows_more() {
    assert!(Span::offering(10.dp()).allows_more());
    assert!(Span::offering(10.dp()).shrunk(4.dp()).allows_more());
    assert!(!Span::offering(Dp::INFINITY).allows_more());
    assert!(!Span::exact(10.dp()).allows_more());
    assert!(!Span::up_to(10.dp()).at_least(4.dp()).allows_more());
    assert!(Span::offering(10.dp()).at_most(20.dp()).allows_more());
    assert!(!Span::offering(10.dp()).at_most(5.dp()).allows_more());
}

#[test]
fn the_bounding_rect_contains_every_point() {
    let points = [
        Point::new(4.dp(), 9.dp()),
        Point::new(-2.dp(), 3.dp()),
        Point::new(6.dp(), 5.dp()),
    ];
    assert_eq!(
        Rect::bounding(points),
        Some(Rect::new(-2.dp(), 3.dp(), 8.dp(), 6.dp()))
    );
    assert_eq!(Rect::bounding([]), None);
}
