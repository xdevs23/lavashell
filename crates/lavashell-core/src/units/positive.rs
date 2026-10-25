// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, NumberError};

const fn checked_positive(number: f64) -> Result<f64, NumberError> {
    if !number.is_finite() {
        Err(NumberError::NotFinite)
    } else if number < 0.0 {
        Err(NumberError::Negative)
    } else if number == 0.0 {
        Err(NumberError::OutOfRange)
    } else {
        Ok(number)
    }
}

/// A finite number above zero.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Positive(f64);

impl Positive {
    /// # Errors
    ///
    /// Fails when `value` is not finite or not above zero.
    pub const fn try_new(value: f64) -> Result<Self, NumberError> {
        match checked_positive(value) {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A finite length above zero.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PositiveDp(Positive);

impl PositiveDp {
    /// # Errors
    ///
    /// Fails when `value` is not finite or not above zero.
    pub const fn try_new(value: Dp) -> Result<Self, NumberError> {
        match Positive::try_new(value.get()) {
            Ok(positive) => Ok(Self(positive)),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn get(self) -> Dp {
        Dp::new(self.0.get())
    }
}
