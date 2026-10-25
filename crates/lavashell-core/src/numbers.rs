// SPDX-License-Identifier: GPL-3.0-only
//! Conversions between number types that can lose precision or fail. Each such conversion is written once here, so
//! every caller converts the same way.

use crate::units::NumberError;
use num_traits::ToPrimitive;
use std::time::Duration;

/// Nanoseconds in one second, by the definition of the prefix.
pub(crate) const NANOS_PER_SECOND: u32 = 1_000_000_000;

/// `count` as an `f64`, which represents every count up to `u32::MAX` exactly.
///
/// # Errors
///
/// Fails for a count above `u32::MAX`.
pub fn count_f64(count: usize) -> Result<f64, NumberError> {
    u32::try_from(count)
        .map(f64::from)
        .map_err(NumberError::Count)
}

/// `value` converted into a count, when `value` is a whole number.
///
/// # Errors
///
/// Fails when `value` is not finite, negative, not whole or above `usize::MAX`.
pub fn whole_count(value: f64) -> Result<usize, NumberError> {
    if !value.is_finite() {
        Err(NumberError::NotFinite)
    } else if value < 0.0 {
        Err(NumberError::Negative)
    } else if value.fract() != 0.0 {
        Err(NumberError::NotWhole)
    } else {
        value.to_usize().ok_or(NumberError::OutOfRange)
    }
}

/// The duration of `nanos` nanoseconds, or `Duration::MAX` when `nanos` is longer. The saturating operations of
/// `Duration` also end at `Duration::MAX`.
pub(crate) fn duration_from_nanos(nanos: u128) -> Duration {
    Duration::from_nanos_u128(nanos.min(Duration::MAX.as_nanos()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestResult;

    #[test]
    fn a_count_converts_exactly_up_to_the_largest_u32_and_fails_beyond() -> TestResult {
        assert_eq!(count_f64(7), Ok(7.0));
        let largest = usize::try_from(u32::MAX)?;
        assert_eq!(count_f64(largest), Ok(f64::from(u32::MAX)));
        assert!(matches!(count_f64(largest + 1), Err(NumberError::Count(_))));
        Ok(())
    }

    #[test]
    fn only_a_finite_whole_number_that_is_not_negative_is_a_count() {
        assert_eq!(whole_count(12.0), Ok(12));
        assert_eq!(whole_count(0.0), Ok(0));
        assert_eq!(whole_count(-1.0), Err(NumberError::Negative));
        assert_eq!(whole_count(2.5), Err(NumberError::NotWhole));
        assert_eq!(whole_count(f64::NAN), Err(NumberError::NotFinite));
        assert_eq!(whole_count(f64::MAX), Err(NumberError::OutOfRange));
    }

    #[test]
    fn nanoseconds_past_the_longest_duration_end_at_it() {
        assert_eq!(
            duration_from_nanos(1_500_000_000),
            Duration::from_millis(1500)
        );
        assert_eq!(duration_from_nanos(u128::MAX), Duration::MAX);
    }
}
