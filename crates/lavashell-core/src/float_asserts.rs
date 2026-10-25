// SPDX-License-Identifier: GPL-3.0-only
//! Test assertions that two f64 values are equal within the gap between neighboring f64 values at the size of the
//! expected value. For an expected value of zero, that gap is the smallest subnormal f64. `closes_with_step`
//! checks that the error of a numeric method shrinks with its step size as the order of the method predicts.

pub(crate) fn same_f64(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= f64::EPSILON * expected.abs().max(f64::MIN_POSITIVE)
}

#[track_caller]
pub(crate) fn assert_f64_eq(actual: f64, expected: f64) {
    assert!(
        same_f64(actual, expected),
        "{actual} differs from {expected}"
    );
}

#[track_caller]
pub(crate) fn assert_f64s_eq(actual: &[f64], expected: &[f64]) {
    assert!(
        actual.len() == expected.len()
            && actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| same_f64(*actual, *expected)),
        "{actual:?} differs from {expected:?}"
    );
}

/// The step sizes at which `closes_with_step` measures a gap, one decade apart. Three step sizes give two ratios,
/// which tell a gap that closes with its step apart from one that does not. The smallest step lies far above the
/// step of about 6e-6 at which the rounding error of a central difference equals its truncation error.
const STEPS: [f64; 3] = [1e-2, 1e-3, 1e-4];

/// Whether `gap`, an error that depends on a step size, shrinks with the step like the truncation error of a method
/// of `order`. Between two steps such an error shrinks by the ratio of the steps to the power `order`. A gap that
/// does not close, such as the error of a wrong derivative, does not shrink. The check requires a shrink by the
/// ratio to half that power, which lies between both cases.
pub(crate) fn closes_with_step(order: f64, mut gap: impl FnMut(f64) -> f64) -> bool {
    let gaps = STEPS.map(&mut gap);
    STEPS
        .windows(2)
        .zip(gaps.windows(2))
        .all(|(steps, gaps)| gaps[1] <= gaps[0] * (steps[1] / steps[0]).powf(order / 2.0))
}
