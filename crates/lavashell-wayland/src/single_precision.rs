// SPDX-License-Identifier: GPL-3.0-only
//! Conversion of the `f64` numbers used by Lavashell to the `f32` coordinates expected by Skia and AccessKit.

use num_traits::AsPrimitive;

/// The `f32` nearest to `value`, or `None` when `value` is finite and beyond the range of `f32`. A value that is not
/// finite converts to the `f32` infinity or NaN of the same kind.
pub(crate) fn nearest(value: f64) -> Option<f32> {
    let narrow: f32 = value.as_();
    (narrow.is_finite() || !value.is_finite()).then_some(narrow)
}

#[cfg(test)]
mod tests {
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
}
