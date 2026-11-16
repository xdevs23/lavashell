// SPDX-License-Identifier: GPL-3.0-only
use super::layout::LayoutError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Currency(&'static str);

impl Currency {
    pub const EURO: Self = Self("€");
    pub const POUND: Self = Self("£");
    pub const DOLLAR: Self = Self("$");
    pub const YEN: Self = Self("¥");
    pub const OFFERED: [Self; 4] = [Self::EURO, Self::POUND, Self::DOLLAR, Self::YEN];

    pub fn new(symbol: &'static str) -> Result<Self, LayoutError> {
        if symbol.is_empty() {
            Err(LayoutError::EmptyCurrency)
        } else {
            Ok(Self(symbol))
        }
    }

    #[must_use]
    pub fn symbol(self) -> &'static str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_currency_needs_a_symbol() {
        assert_eq!(Currency::new(""), Err(LayoutError::EmptyCurrency));
        assert_eq!(Currency::new("R$").map(Currency::symbol), Ok("R$"));
    }

    #[test]
    fn every_offered_currency_has_a_single_character_for_its_key() {
        for currency in Currency::OFFERED {
            assert_eq!(currency.symbol().chars().count(), 1, "{currency:?}");
        }
    }
}
