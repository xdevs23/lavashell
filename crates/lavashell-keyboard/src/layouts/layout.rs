// SPDX-License-Identifier: GPL-3.0-only

use super::currency::Currency;
use crate::board::MIN_TOP_ROW;
use icu_locale_core::LanguageIdentifier;
use icu_locale_core::subtags::Language;
use lavashell_core::numbers::count_f64;
use lavashell_core::units::NumberError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutSpec {
    pub id: &'static str,
    pub language: Language,
    pub name: &'static str,
    pub arrangement: &'static str,
    pub rows: [&'static str; 3],
    pub further_letters: &'static [(char, &'static str)],
    pub currency: Currency,
}

#[derive(Debug)]
pub struct Layout {
    pub(super) spec: LayoutSpec,
}

impl PartialEq for Layout {
    fn eq(&self, other: &Self) -> bool {
        self.spec.id == other.spec.id
    }
}

impl Eq for Layout {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("the top row has fewer than {fewest} letters", fewest = MIN_TOP_ROW)]
    ShortTopRow,
    #[error("a lower row is too long for the top row")]
    LongRow,
    #[error("no key types {0}")]
    LetterWithoutKey(char),
    #[error("the letters of the top row are too many to count exactly")]
    CountTopRow(#[source] NumberError),
    #[error("a currency needs a symbol")]
    EmptyCurrency,
}

impl Layout {
    pub fn new(spec: LayoutSpec) -> Result<Self, LayoutError> {
        let [top, middle, third] = spec.rows.map(|row| row.chars().count());
        if count_f64(top).map_err(LayoutError::CountTopRow)? < MIN_TOP_ROW {
            return Err(LayoutError::ShortTopRow);
        }
        if middle > top || third >= top {
            return Err(LayoutError::LongRow);
        }
        let layout = Self { spec };
        match (spec.further_letters.iter()).find(|(base, _)| !layout.has_key(*base)) {
            Some((base, _)) => Err(LayoutError::LetterWithoutKey(*base)),
            None => Ok(layout),
        }
    }

    #[must_use]
    pub fn id(&self) -> &'static str {
        self.spec.id
    }

    #[must_use]
    pub fn language(&self) -> Language {
        self.spec.language
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        self.spec.name
    }

    #[must_use]
    pub fn arrangement(&self) -> &'static str {
        self.spec.arrangement
    }

    #[must_use]
    pub fn currency(&self) -> Currency {
        self.spec.currency
    }

    pub(super) fn written_in(&self, locale: &LanguageIdentifier) -> bool {
        locale.language == self.spec.language
    }

    pub(crate) fn rows(&self) -> [&'static str; 3] {
        self.spec.rows
    }

    pub(crate) fn has_key(&self, letter: char) -> bool {
        self.spec.rows.iter().any(|row| row.contains(letter))
    }

    #[must_use]
    pub fn further_letters_of(&self, letter: char) -> Option<&'static str> {
        (self.spec.further_letters.iter())
            .find(|(base, _)| *base == letter)
            .map(|(_, letters)| *letters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icu_locale_core::subtags::language;
    fn abc(
        rows: [&'static str; 3],
        further_letters: &'static [(char, &'static str)],
    ) -> LayoutSpec {
        LayoutSpec {
            id: "xx-abc",
            language: language!("xx"),
            name: "X",
            arrangement: "ABC",
            rows,
            further_letters,
            currency: Currency::DOLLAR,
        }
    }

    #[test]
    fn a_layout_whose_rows_do_not_fit_the_keys_is_refused() {
        let letters: &[(char, &str)] = &[('x', "ẋ")];
        let layout = |rows| Layout::new(abc(rows, &[]));
        assert_eq!(
            layout(["abcdef", "ghi", "jk"]),
            Err(LayoutError::ShortTopRow)
        );
        assert_eq!(
            layout(["abcdefg", "hijklmno", "pq"]),
            Err(LayoutError::LongRow)
        );
        assert_eq!(
            layout(["abcdefg", "hijklmn", "opqrstu"]),
            Err(LayoutError::LongRow)
        );
        assert_eq!(
            Layout::new(abc(["abcdefg", "hij", "klm"], letters)),
            Err(LayoutError::LetterWithoutKey('x'))
        );
    }
}
