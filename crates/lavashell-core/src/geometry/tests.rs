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

#[test]
fn a_size_converts_only_when_both_lengths_are_in_range() {
    let size = |width: f64, height: f64| Size::new(Dp::new(width), Dp::new(height));
    assert!(size(0.3, 2.0).to_positive().is_ok());
    assert_eq!(size(0.0, 2.0).to_positive(), Err(NumberError::OutOfRange));
    assert_eq!(
        size(2.0, f64::NAN).to_positive(),
        Err(NumberError::NotFinite)
    );
    assert!(size(0.0, 0.0).to_non_negative().is_ok());
    assert_eq!(
        size(2.0, -1.0).to_non_negative(),
        Err(NumberError::Negative)
    );
    assert_eq!(
        size(f64::INFINITY, 2.0).to_non_negative(),
        Err(NumberError::NotFinite)
    );
}
