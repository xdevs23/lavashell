// SPDX-License-Identifier: GPL-3.0-only
use crate::units::NumberError;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontWeight(f64);

impl FontWeight {
    // https://www.w3.org/TR/2026/WD-css-fonts-4-20261112/#font-weight-numeric-values
    pub const THIN: Self = crate::constant!(Self::try_new(100.0));
    pub const EXTRA_LIGHT: Self = crate::constant!(Self::try_new(200.0));
    pub const LIGHT: Self = crate::constant!(Self::try_new(300.0));
    pub const NORMAL: Self = crate::constant!(Self::try_new(400.0));
    pub const MEDIUM: Self = crate::constant!(Self::try_new(500.0));
    pub const SEMI_BOLD: Self = crate::constant!(Self::try_new(600.0));
    pub const BOLD: Self = crate::constant!(Self::try_new(700.0));
    pub const EXTRA_BOLD: Self = crate::constant!(Self::try_new(800.0));
    pub const BLACK: Self = crate::constant!(Self::try_new(900.0));

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

    // https://www.w3.org/TR/2026/WD-css-fonts-4-20261112/#font-weight-numeric-values
    const LIGHTEST: f64 = 1.0;
    const HEAVIEST: f64 = 1000.0;

    pub const fn try_new(weight: f64) -> Result<Self, NumberError> {
        if !weight.is_finite() {
            Err(NumberError::NotFinite)
        } else if weight >= Self::LIGHTEST && weight <= Self::HEAVIEST {
            Ok(Self(weight))
        } else {
            Err(NumberError::OutOfRange)
        }
    }

    pub const fn saturating(weight: f64) -> Result<Self, NumberError> {
        if weight.is_finite() {
            Ok(Self::clamp_to_range(weight))
        } else {
            Err(NumberError::NotFinite)
        }
    }

    #[must_use]
    pub fn saturating_whole(weight: i32) -> Self {
        Self::clamp_to_range(f64::from(weight))
    }

    const fn clamp_to_range(finite_weight: f64) -> Self {
        Self(finite_weight.clamp(Self::LIGHTEST, Self::HEAVIEST))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Eq for FontWeight {}

impl Ord for FontWeight {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for FontWeight {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

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

    #[test]
    fn a_saturated_weight_stops_at_the_nearest_end_and_fails_only_when_not_finite() {
        assert_eq!(FontWeight::saturating(0.5), Ok(FontWeight(1.0)));
        assert_eq!(FontWeight::saturating(550.0), Ok(FontWeight(550.0)));
        assert_eq!(FontWeight::saturating(1040.0), Ok(FontWeight(1000.0)));
        assert_eq!(
            FontWeight::saturating(f64::INFINITY),
            Err(NumberError::NotFinite)
        );
    }

    #[test]
    fn a_whole_weight_stops_at_the_nearest_end() {
        assert_eq!(FontWeight::saturating_whole(0), FontWeight(1.0));
        assert_eq!(FontWeight::saturating_whole(550), FontWeight(550.0));
        assert_eq!(FontWeight::saturating_whole(i32::MAX), FontWeight(1000.0));
    }

    #[test]
    fn weights_order_from_lightest_to_heaviest() {
        let mut weights = [FontWeight::BOLD, FontWeight::THIN, FontWeight::NORMAL];
        weights.sort_unstable();
        assert_eq!(
            weights,
            [FontWeight::THIN, FontWeight::NORMAL, FontWeight::BOLD]
        );
    }
}
