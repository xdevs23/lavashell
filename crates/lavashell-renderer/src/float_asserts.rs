// SPDX-License-Identifier: GPL-3.0-only
//! Assertions that floats are equal up to the spacing of floats at the size of the numbers they are computed from.
//! At zero, that spacing is the smallest subnormal step.

use num_traits::Float;

/// Whether `actual` is `expected` up to the spacing of floats of their type at `size`.
pub(crate) fn same_at<F: Float>(actual: F, expected: F, size: F) -> bool {
    (actual - expected).abs() <= F::epsilon() * size.abs().max(F::min_positive_value())
}

#[track_caller]
pub(crate) fn assert_f32_eq(actual: f32, expected: f32) {
    assert!(
        same_at(actual, expected, expected),
        "{actual} differs from {expected}"
    );
}
