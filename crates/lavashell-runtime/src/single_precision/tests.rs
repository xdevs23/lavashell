// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn a_finite_value_beyond_the_range_of_f32_has_no_nearest_f32() {
    assert_eq!(nearest(f64::MAX), None);
    assert_eq!(nearest(-f64::MAX), None);
}

#[test]
fn a_value_inside_the_range_or_not_finite_narrows() {
    assert_eq!(nearest(0.5), Some(0.5));
    assert_eq!(nearest(f64::from(f32::MAX)), Some(f32::MAX));
    assert_eq!(nearest(f64::INFINITY), Some(f32::INFINITY));
    assert!(nearest(f64::NAN).is_some_and(f32::is_nan));
}
