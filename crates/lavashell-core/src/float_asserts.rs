// SPDX-License-Identifier: GPL-3.0-only

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

const STEPS: [f64; 3] = [1e-2, 1e-3, 1e-4];

pub(crate) fn closes_with_step(order: f64, mut gap: impl FnMut(f64) -> f64) -> bool {
    let gaps = STEPS.map(&mut gap);
    STEPS
        .windows(2)
        .zip(gaps.windows(2))
        .all(|(steps, gaps)| gaps[1] <= gaps[0] * (steps[1] / steps[0]).powf(order / 2.0))
}
