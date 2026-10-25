// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{Easing, Motion, Tween};
use crate::float_asserts::assert_f64_eq;
use crate::geometry::{Length, Point, Size};
use crate::test_error::{Required, TestResult};
use std::time::Duration;

#[test]
fn lengths_add_subtract_scale_and_divide_within_their_unit() {
    let mut length = Dp::new(10.0) + Dp::new(6.0) - Dp::new(4.0);
    assert_eq!(length, Dp::new(12.0));
    assert_eq!(-length, Dp::new(-12.0));
    assert_eq!(length * 2.0, Dp::new(24.0));
    assert_eq!(0.5 * length, Dp::new(6.0));
    assert_eq!(length / 4.0, Dp::new(3.0));
    assert_f64_eq(length / Dp::new(4.0), 3.0);
    length += Dp::new(3.0);
    length -= Dp::new(1.0);
    length *= 3.0;
    length /= 2.0;
    assert_eq!(length, Dp::new(21.0));
}

#[test]
fn lengths_compare_clamp_round_and_sum() {
    assert!(Dp::new(1.0) < Dp::new(2.0));
    assert_eq!(Dp::new(1.0).min(Dp::new(2.0)), Dp::new(1.0));
    assert_eq!(Dp::new(1.0).max(Dp::new(2.0)), Dp::new(2.0));
    assert_eq!(Dp::new(5.0).clamp(Dp::new(0.0), Dp::new(4.0)), Dp::new(4.0));
    assert_eq!(Dp::new(-3.0).abs(), Dp::new(3.0));
    assert_eq!(Dp::new(2.5).floor(), Dp::new(2.0));
    assert_eq!(Dp::new(2.1).ceil(), Dp::new(3.0));
    assert_eq!(Dp::new(2.5).round(), Dp::new(3.0));
    assert_eq!(Dp::new(-2.5).round(), Dp::new(-3.0));
    assert_eq!(Dp::new(3.0).hypot(Dp::new(4.0)), Dp::new(5.0));
    assert!(Dp::MAX.is_finite() && !Dp::new(f64::NAN).is_finite());
    let lengths = [Dp::new(1.0), Dp::new(2.0), Dp::new(3.5)];
    assert_eq!(lengths.iter().sum::<Dp>(), Dp::new(6.5));
    assert_eq!(lengths.into_iter().sum::<Dp>(), Dp::new(6.5));
}

#[test]
fn a_density_converts_dp_to_px_and_back() -> TestResult {
    let density = Density::from_dpi(320.0)?;
    assert_f64_eq(density.get(), 2.0);
    assert_eq!(Dp::new(16.0) * density, Px::new(32.0));
    assert_eq!(Px::new(32.0) / density, Dp::new(16.0));
    assert_eq!(density.to_px(Dp::new(16.0)), Px::new(32.0));
    assert_eq!(density.to_dp(Px::new(32.0)), Dp::new(16.0));
    assert_eq!(Density::from_dpi(160.0)?, Density::ONE);
    assert_f64_eq(Density::from_dpi(213.0)?.get(), 213.0 / 160.0);
    assert_f64_eq(Density::from_ratio(180.0, 120.0)?.get(), 1.5);
    assert_f64_eq(Density::from_scale(1.5).required("density")?.get(), 1.5);
    assert_eq!(
        Point::from_px(Px::new(4.0), Px::new(6.0), density),
        Point::new(Dp::new(2.0), Dp::new(3.0))
    );
    assert_eq!(
        Size::from_px(Px::new(8.0), Px::new(10.0), density),
        Size::new(Dp::new(4.0), Dp::new(5.0))
    );
    Ok(())
}

#[test]
fn a_buffer_covers_every_dp_of_a_size_in_whole_pixels() -> TestResult {
    let density = Density::from_scale(1.25).required("density")?;
    let size = Size::new(Dp::new(1401.0), Dp::new(0.3));
    assert_eq!(size.buffer_size(density), Ok((1752, 1)));
    assert_eq!(Size::default().buffer_size(Density::ONE), Ok((0, 0)));
    Ok(())
}

#[test]
fn a_buffer_of_a_negative_length_is_rejected() {
    let size = Size::new(Dp::new(-1.0), Dp::new(1.0));
    assert_eq!(size.buffer_size(Density::ONE), Err(NumberError::Negative));
}

#[test]
fn a_buffer_of_a_length_that_is_not_finite_is_rejected() {
    let size = Size::new(Dp::new(f64::NAN), Dp::new(1.0));
    assert_eq!(size.buffer_size(Density::ONE), Err(NumberError::NotFinite));
}

#[test]
fn a_buffer_longer_than_its_pixel_count_holds_is_rejected() {
    let size = Size::new(Dp::new(1.0), Dp::new(f64::from(u32::MAX) * 2.0));
    assert_eq!(size.buffer_size(Density::ONE), Err(NumberError::OutOfRange));
}

#[test]
fn a_density_rejects_zero_negative_and_non_finite_scales() {
    for scale in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Density::from_scale(scale), None, "{scale}");
    }
}

#[test]
fn a_density_refuses_a_ratio_or_dpi_that_is_not_finite_and_above_zero() {
    let refused = [
        (0.0, 1.0, NumberError::OutOfRange),
        (1.0, 0.0, NumberError::OutOfRange),
        (-1.0, -1.0, NumberError::Negative),
        (f64::NAN, 1.0, NumberError::NotFinite),
        (1.0, f64::INFINITY, NumberError::NotFinite),
        (f64::MAX, 0.5, NumberError::NotFinite),
        (
            f64::MIN_POSITIVE * f64::EPSILON,
            2.0,
            NumberError::OutOfRange,
        ),
    ];
    for (pixels, dp, error) in refused {
        assert_eq!(
            Density::from_ratio(pixels, dp),
            Err(error),
            "{pixels} / {dp}"
        );
    }
    assert_eq!(Density::from_dpi(-160.0), Err(NumberError::Negative));
}

#[test]
fn unsuffixed_integer_and_float_literals_become_units() {
    const PADDING: Dp = Dp::new(12.0);
    assert_eq!(16.dp(), Dp::new(16.0));
    assert_eq!(0.5.dp(), Dp::new(0.5));
    assert_eq!((-8).dp(), Dp::new(-8.0));
    assert_eq!(PADDING, 12.dp());
    assert_eq!(3.px(), Px::new(3.0));
    assert_eq!(14.sp(), Sp::new(14.0));
    assert_eq!(0.25.em(), Em::new(0.25));
}

#[test]
fn letter_spacing_in_em_is_a_fraction_of_the_font_size() {
    assert_eq!(0.25.em().to_dp(16.dp()), 4.dp());
    assert_eq!((-0.5).em().to_dp(16.dp()), (-8).dp());
}

#[test]
fn dp_animates_like_a_scalar() -> TestResult {
    let motion = Motion::default();
    let width = motion.animation(0.dp())?;
    let linear = Tween::new(Duration::from_millis(100))
        .easing(Easing::Linear)
        .preserve_velocity(false);
    width.animate_to(100.dp(), linear)?;
    motion.advance(Duration::from_millis(50));
    assert_eq!(width.get(), 50.dp());
    motion.advance(Duration::from_millis(100));
    assert_eq!(width.get(), 100.dp());
    Ok(())
}

#[test]
fn checked_numbers_keep_valid_values_and_name_what_is_wrong() {
    assert_eq!(NonNegative::try_new(2.5).map(NonNegative::get), Ok(2.5));
    assert_eq!(NonNegative::try_new(-0.5), Err(NumberError::Negative));
    for value in [f64::NAN, f64::INFINITY] {
        assert_eq!(NonNegative::try_new(value), Err(NumberError::NotFinite));
        assert_eq!(
            FiniteDp::try_new(Dp::new(value)),
            Err(NumberError::NotFinite)
        );
    }
    assert_eq!(NonNegative::saturating(-0.5), Ok(NonNegative::ZERO));
    assert_eq!(Fraction::try_new(0.4).map(Fraction::get), Ok(0.4));
    assert_eq!(Fraction::try_new(-0.1), Err(NumberError::OutOfRange));
    assert_eq!(Fraction::try_new(1.1), Err(NumberError::OutOfRange));
    assert_eq!(Fraction::try_new(f64::NAN), Err(NumberError::NotFinite));
}

#[test]
fn checked_lengths_keep_their_unit() {
    assert_eq!(
        NonNegativeDp::try_new(2.dp()).map(NonNegativeDp::get),
        Ok(2.dp())
    );
    assert_eq!(
        NonNegativeDp::try_new((-2).dp()),
        Err(NumberError::Negative)
    );
    assert_eq!(
        NonNegativeDp::saturating((-2).dp()),
        Ok(NonNegativeDp::ZERO)
    );
    assert_eq!(
        FiniteDp::try_new((-3).dp()).map(FiniteDp::get),
        Ok((-3).dp())
    );
}

#[test]
fn checked_numbers_can_be_made_in_constants() {
    const SPREAD: FiniteDp = crate::constant!(FiniteDp::try_new(Dp::new(-4.0)));
    const WIDTH: NonNegativeDp = crate::constant!(NonNegativeDp::saturating(Dp::new(-1.0)));
    assert_eq!((SPREAD.get(), WIDTH), ((-4).dp(), NonNegativeDp::ZERO));
}

#[test]
fn saturating_a_negative_infinity_fails() {
    assert_eq!(
        NonNegativeDp::saturating(Dp::new(f64::NEG_INFINITY)),
        Err(NumberError::NotFinite)
    );
}

#[test]
fn a_halved_length_stays_at_least_zero() {
    let length = crate::constant!(NonNegativeDp::try_new(Dp::new(8.0)));
    assert_eq!(length.halved(0), length);
    assert_eq!(length.halved(2).get(), 2.dp());
    assert!(length.halved(u8::MAX) > NonNegativeDp::ZERO);
}

#[test]
fn a_length_is_dp_or_percent_of_the_viewport_or_the_box() {
    let viewport = Size::new(400.dp(), 200.dp());
    let extent = 50.dp();
    assert_eq!(Length::from(7.dp()).to_dp(extent, viewport), 7.dp());
    assert_eq!(Length::from(10.vw()).to_dp(extent, viewport), 40.dp());
    assert_eq!(Length::from(-32.5.vh()).to_dp(extent, viewport), (-65).dp());
    assert_eq!(Length::from(50.pct()).to_dp(extent, viewport), 25.dp());
}

#[test]
fn a_scale_factor_is_only_made_within_its_range() {
    let (smallest, largest) = (*ScaleFactor::RANGE.start(), *ScaleFactor::RANGE.end());
    for scale in [f64::NAN, f64::INFINITY] {
        assert_eq!(ScaleFactor::try_new(scale), Err(NumberError::NotFinite));
    }
    for scale in [0.0, -1.0, smallest / 2.0, largest * 2.0] {
        assert_eq!(ScaleFactor::try_new(scale), Err(NumberError::OutOfRange));
    }
    for scale in [smallest, 1.0, largest] {
        assert_eq!(ScaleFactor::try_new(scale).map(ScaleFactor::get), Ok(scale));
    }
}

#[test]
fn a_saturating_fraction_is_the_nearest_value_within_its_range() -> TestResult {
    assert_f64_eq(Fraction::saturating(-0.5)?.get(), 0.0);
    assert_f64_eq(Fraction::saturating(0.25)?.get(), 0.25);
    assert_f64_eq(Fraction::saturating(f64::INFINITY)?.get(), 1.0);
    assert_eq!(Fraction::saturating(f64::NAN), Err(NumberError::NotFinite));
    Ok(())
}

#[test]
fn a_positive_number_is_finite_and_above_zero() {
    assert_eq!(Positive::try_new(2.5).map(Positive::get), Ok(2.5));
    assert_eq!(Positive::try_new(0.0), Err(NumberError::OutOfRange));
    assert_eq!(Positive::try_new(-1.0), Err(NumberError::Negative));
    for value in [f64::NAN, f64::INFINITY] {
        assert_eq!(Positive::try_new(value), Err(NumberError::NotFinite));
    }
}

#[test]
fn a_positive_length_is_finite_and_above_zero() {
    assert_eq!(
        PositiveDp::try_new(Dp::new(2.5)).map(PositiveDp::get),
        Ok(Dp::new(2.5))
    );
    assert_eq!(PositiveDp::try_new(Dp::ZERO), Err(NumberError::OutOfRange));
    assert_eq!(
        PositiveDp::try_new(Dp::new(-1.0)),
        Err(NumberError::Negative)
    );
    assert_eq!(
        PositiveDp::try_new(Dp::INFINITY),
        Err(NumberError::NotFinite)
    );
}
