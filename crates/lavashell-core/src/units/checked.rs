// SPDX-License-Identifier: GPL-3.0-only
use super::Dp;
use std::num::TryFromIntError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NumberError {
    #[error("the number must not be negative")]
    Negative,
    #[error("the number must be finite")]
    NotFinite,
    /// The number is finite, but lies outside the range accepted by the checked type, such as a fraction above one.
    #[error("the number is outside the range of its type")]
    OutOfRange,
    #[error("the number must be whole")]
    NotWhole,
    #[error("the count is too large to convert exactly")]
    Count(#[source] TryFromIntError),
}

const fn checked_non_negative(number: f64) -> Result<f64, NumberError> {
    if !number.is_finite() {
        Err(NumberError::NotFinite)
    } else if number < 0.0 {
        Err(NumberError::Negative)
    } else {
        Ok(number)
    }
}

const fn checked_finite(number: f64) -> Result<f64, NumberError> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(NumberError::NotFinite)
    }
}

const fn checked_fraction(number: f64) -> Result<f64, NumberError> {
    if !number.is_finite() {
        Err(NumberError::NotFinite)
    } else if number < 0.0 || number > 1.0 {
        Err(NumberError::OutOfRange)
    } else {
        Ok(number)
    }
}

const fn plain(number: f64) -> f64 {
    number
}

/// Defines the type `$name`, which wraps an `f64` accepted by `$check`. `try_new` converts a `$value` to that
/// `f64` with `$number` before the check, and `get` converts the `f64` back to a `$value` with `$from_number`.
macro_rules! checked {
    ($name:ident($value:ty), $check:path, $number:path, $from_number:path) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
        pub struct $name(f64);
        impl $name {
            pub const ZERO: Self = Self(0.0);
            /// # Errors
            ///
            /// Fails when `value` is not finite or lies outside the range of the type.
            pub const fn try_new(value: $value) -> Result<Self, NumberError> {
                match $check($number(value)) {
                    Ok(number) => Ok(Self(number)),
                    Err(error) => Err(error),
                }
            }
            pub const fn get(self) -> $value {
                $from_number(self.0)
            }
        }
        impl From<$name> for $value {
            fn from(checked: $name) -> Self {
                checked.get()
            }
        }
    };
}

macro_rules! saturating {
    ($($name:ident($value:ty)),*) => {$(
        impl $name {
            /// The number made from `value`, or zero when `value` is negative.
            ///
            /// # Errors
            ///
            /// Fails when `value` is not finite.
            pub const fn saturating(value: $value) -> Result<Self, NumberError> {
                match Self::try_new(value) {
                    Err(NumberError::Negative) => Ok(Self::ZERO),
                    checked => checked,
                }
            }
        }
    )*};
}

checked!(NonNegative(f64), checked_non_negative, plain, plain);

checked!(NonNegativeDp(Dp), checked_non_negative, Dp::get, Dp::new);

checked!(Fraction(f64), checked_fraction, plain, plain);

checked!(FiniteDp(Dp), checked_finite, Dp::get, Dp::new);

saturating!(NonNegative(f64), NonNegativeDp(Dp));

impl NonNegativeDp {
    /// This length divided by two `times` times. The result is finite and not negative, like this length.
    #[must_use]
    pub fn halved(self, times: u8) -> Self {
        Self(self.0 * 0.5_f64.powi(i32::from(times)))
    }
}

impl Fraction {
    /// The fraction nearest to `value`: zero for a value below zero and one for a value above one.
    ///
    /// # Errors
    ///
    /// Fails when `value` is NaN.
    pub fn saturating(value: f64) -> Result<Self, NumberError> {
        Self::try_new(value.clamp(0.0, 1.0))
    }
}
