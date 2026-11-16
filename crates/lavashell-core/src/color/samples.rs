// SPDX-License-Identifier: GPL-3.0-only
use super::{Color, DEGREES_PER_TURN, Gamut, HuePath};
use crate::float_asserts::assert_f64_eq;
use crate::output::finest_step;

pub const BLUE: Color = Color::oklch(0.574, 0.195, 258.0);
pub const VIOLET: Color = Color::oklch(0.514, 0.226, 303.0);
pub const GREEN: Color = Color::oklch(0.527, 0.141, 148.0);

const HALF_TURN: f64 = DEGREES_PER_TURN / 2.0;

pub fn hue_distance(a: f64, b: f64) -> f64 {
    ((a - b + HALF_TURN).rem_euclid(DEGREES_PER_TURN) - HALF_TURN).abs()
}

pub fn whole_degree_hues() -> impl Iterator<Item = f64> {
    std::iter::successors(Some(0.0), |hue| Some(hue + 1.0))
        .take_while(|hue| *hue < DEGREES_PER_TURN)
}

pub fn hue_spacing() -> f64 {
    f64::EPSILON * DEGREES_PER_TURN
}

pub fn assert_halfway_from_blue_to_violet(color: Color, path: HuePath) {
    let ([l0, c0, h0], [l1, c1, h1]) = (BLUE.to_oklch(), VIOLET.to_oklch());
    let shorter = f64::midpoint(h0, h1);
    let hue = match path {
        HuePath::Shorter | HuePath::Increasing => shorter,
        HuePath::Longer | HuePath::Decreasing => shorter + HALF_TURN,
    };
    let [l, c, h] = color.to_oklch();
    assert_f64_eq(l, f64::midpoint(l0, l1));
    assert_f64_eq(c, f64::midpoint(c0, c1));
    assert!(
        hue_distance(h, hue) <= hue_spacing(),
        "{h} differs from {hue}"
    );
}

#[track_caller]
pub fn assert_shown_alike(actual: Color, expected: Color) {
    let numbers = |color: Color, gamut| color.to_encoded(gamut).into_iter().chain([color.alpha()]);
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        let mut differences =
            (numbers(actual, gamut).zip(numbers(expected, gamut))).map(|(a, e)| (a - e).abs());
        assert!(
            differences.all(|difference| difference <= finest_step() / 2.0),
            "{actual:?} differs from {expected:?} in {gamut:?}"
        );
    }
}
