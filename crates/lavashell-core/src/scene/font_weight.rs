// SPDX-License-Identifier: GPL-3.0-only
use crate::units::NumberError;
use std::hash::{Hash, Hasher};

/// The weight of a font, from 1 to 1000 as CSS Fonts Level 4 defines `font-weight`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct FontWeight(f64);

impl FontWeight {
    pub const THIN: Self = crate::constant!(Self::try_new(100.0));
    pub const EXTRA_LIGHT: Self = crate::constant!(Self::try_new(200.0));
    pub const LIGHT: Self = crate::constant!(Self::try_new(300.0));
    pub const NORMAL: Self = crate::constant!(Self::try_new(400.0));
    pub const MEDIUM: Self = crate::constant!(Self::try_new(500.0));
    pub const SEMI_BOLD: Self = crate::constant!(Self::try_new(600.0));
    pub const BOLD: Self = crate::constant!(Self::try_new(700.0));
    pub const EXTRA_BOLD: Self = crate::constant!(Self::try_new(800.0));
    pub const BLACK: Self = crate::constant!(Self::try_new(900.0));

    /// The weights that CSS names, from the lightest to the heaviest.
    pub const NAMED: [Self; 9] = [
        Self::THIN,
        Self::EXTRA_LIGHT,
        Self::LIGHT,
        Self::NORMAL,
        Self::MEDIUM,
        Self::SEMI_BOLD,
        Self::BOLD,
        Self::EXTRA_BOLD,
        Self::BLACK,
    ];

    /// # Errors
    ///
    /// Fails when `weight` is not finite or lies outside 1 to 1000.
    pub const fn try_new(weight: f64) -> Result<Self, NumberError> {
        if !weight.is_finite() {
            Err(NumberError::NotFinite)
        } else if weight >= 1.0 && weight <= 1000.0 {
            Ok(Self(weight))
        } else {
            Err(NumberError::OutOfRange)
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

// A weight is never NaN, so equality is total.
impl Eq for FontWeight {}

// Every weight lies between 1 and 1000, a range without zero or NaN. Two equal weights in that range have the same
// bits, so hashing the bits agrees with equality.
impl Hash for FontWeight {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

#[cfg(feature = "serde")]
crate::encoding::checked_serde!(FontWeight(f64));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weight_lies_between_1_and_1000() {
        assert_eq!(FontWeight::try_new(1.0), Ok(FontWeight(1.0)));
        assert_eq!(FontWeight::try_new(1000.0), Ok(FontWeight(1000.0)));
        assert_eq!(FontWeight::try_new(0.5), Err(NumberError::OutOfRange));
        assert_eq!(FontWeight::try_new(1000.5), Err(NumberError::OutOfRange));
        assert_eq!(FontWeight::try_new(f64::NAN), Err(NumberError::NotFinite));
    }
}
