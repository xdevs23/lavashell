// SPDX-License-Identifier: GPL-3.0-only

use num_traits::Float;

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
