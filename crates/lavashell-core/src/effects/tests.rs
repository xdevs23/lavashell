// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::float_asserts::assert_f64_eq;
use crate::output::Precision;
use crate::test_error::TestResult;
use crate::units::DpExt;

#[test]
fn builders_fail_on_a_negative_amount() -> TestResult {
    assert_eq!(
        Stroke::new(Color::WHITE, -2.dp()),
        Err(NumberError::Negative)
    );
    let shadow = Shadow::new(Color::WHITE, 4.dp(), Point::new(-3.dp(), 2.dp()))?;
    assert_eq!(shadow.offset.get(), Point::new(-3.dp(), 2.dp()));
    let offset = Point::new(Dp::ZERO, Dp::ZERO);
    assert_eq!(
        Shadow::new(Color::WHITE, -4.dp(), offset),
        Err(NumberError::Negative)
    );
    assert_eq!(Backdrop::blur(-1.dp()), Err(NumberError::Negative));
    let glass = Glass {
        refraction: NonNegativeDp::ZERO,
        dispersion: NonNegativeDp::ZERO,
        rim: NonNegative::ZERO,
    };
    assert_eq!(Backdrop::glass(-1.dp(), glass), Err(NumberError::Negative));
    Ok(())
}

#[test]
fn builders_fail_on_a_number_that_is_not_finite() -> TestResult {
    let not_finite = Dp::new(f64::NAN);
    assert_eq!(
        Stroke::new(Color::WHITE, not_finite),
        Err(NumberError::NotFinite)
    );
    let offset = Point::new(Dp::ZERO, not_finite);
    let shadow = Shadow::new(Color::WHITE, Dp::ZERO, offset);
    assert_eq!(shadow, Err(NumberError::NotFinite));
    let sharp = Shadow::new(Color::WHITE, Dp::ZERO, Point::default())?;
    assert_eq!(sharp.spread(not_finite), Err(NumberError::NotFinite));
    assert_eq!(Backdrop::blur(not_finite), Err(NumberError::NotFinite));
    Ok(())
}

#[test]
fn a_blur_reaches_less_far_on_a_coarser_target_and_no_farther_than_on_the_deepest_output() {
    let deepest = blur_reach(finest_step());
    let eight_bits = blur_reach(Precision::Eight.step());
    assert!(eight_bits < deepest, "{eight_bits} is not below {deepest}");
    assert_f64_eq(blur_reach(f64::EPSILON), deepest);
}
