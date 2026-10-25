// SPDX-License-Identifier: GPL-3.0-only
//! The layouts shipped with the keyboard. The accented letters listed under each plain letter
//! come in the order of how often a writer of the language needs them.

use crate::board::MIN_TOP_ROW;
use icu_locale_core::LanguageIdentifier;
use icu_locale_core::subtags::{Language, language};
use lavashell_core::numbers::count_f64;
use lavashell_core::units::NumberError;

/// The description of a keyboard layout, checked by `Layout::new`: its identifier and names, the
/// letters on its keys, the accented letters shown on a long press and its currency symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutSpec {
    /// The identifier by which `Layout::find` finds the layout, such as `de-qwertz`.
    pub id: &'static str,
    /// The language written with the layout, such as `de`. Among the configured layouts, the
    /// keyboard offers those whose language is the language of a locale of the platform.
    pub language: Language,
    /// The name of the layout in that language.
    pub name: &'static str,
    /// The name of the arrangement of the letters, such as `QWERTZ`.
    pub arrangement: &'static str,
    /// The three rows of letter keys from top to bottom. The top row has the most keys.
    pub rows: [&'static str; 3],
    /// The accented letters of the language, each listed under the plain letter on a key, such as
    /// `ß` under `s`. A long press on the plain letter shows them on whichever offered layout is
    /// shown, except the letters that have their own key there.
    pub further_letters: &'static [(char, &'static str)],
    /// The text typed by the currency key on the first page of symbols, such as `€`.
    pub currency: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    spec: LayoutSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    /// The top row has too few letters. The bottom row spans the same columns as the top row and
    /// needs room for the keys beside the space bar and at least one column of space bar.
    #[error("the top row has fewer than {fewest} letters", fewest = MIN_TOP_ROW)]
    ShortTopRow,
    /// The middle row is longer than the top row, or the third row is as long as the top row or
    /// longer.
    #[error("a lower row is too long for the top row")]
    LongRow,
    /// The layout lists accented letters under a plain letter, but no key types the plain letter.
    #[error("no key types {0}")]
    LetterWithoutKey(char),
    #[error("the letters of the top row are too many to count exactly")]
    CountTopRow(#[source] NumberError),
}

impl Layout {
    /// Checks `spec` and returns the layout described by it.
    ///
    /// # Errors
    ///
    /// Fails when the top row is too short for the bottom row, when a lower row is too long, or
    /// when a plain letter that lists accented letters has no key.
    pub fn new(spec: LayoutSpec) -> Result<Self, LayoutError> {
        let [top, middle, third] = spec.rows.map(|row| row.chars().count());
        if count_f64(top).map_err(LayoutError::CountTopRow)? < MIN_TOP_ROW {
            return Err(LayoutError::ShortTopRow);
        }
        if middle > top || third >= top {
            return Err(LayoutError::LongRow);
        }
        let layout = Self::shipped(spec);
        match (spec.further_letters.iter()).find(|(base, _)| !layout.has_key(*base)) {
            Some((base, _)) => Err(LayoutError::LetterWithoutKey(*base)),
            None => Ok(layout),
        }
    }

    /// Builds the layout without the checks of `new`, because a `static` can only call a `const`
    /// function. The layouts in this module are built with it, and a test checks each of them
    /// with `new`.
    const fn shipped(spec: LayoutSpec) -> Self {
        Self { spec }
    }

    #[must_use]
    pub fn find(id: &str) -> Option<&'static Layout> {
        LAYOUTS.iter().copied().find(|layout| layout.id() == id)
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
    pub fn currency(&self) -> &'static str {
        self.spec.currency
    }

    /// Whether the language subtag of `locale` is the language of the layout. The region and the
    /// script do not matter: `pt-BR` matches a `pt` layout.
    fn written_in(&self, locale: &LanguageIdentifier) -> bool {
        locale.language == self.spec.language
    }

    pub(crate) fn rows(&self) -> [&'static str; 3] {
        self.spec.rows
    }

    pub(crate) fn has_key(&self, letter: char) -> bool {
        self.spec.rows.iter().any(|row| row.contains(letter))
    }

    /// The accented letters listed under the plain `letter` for a long press, in lower case, or
    /// `None` when the layout lists none under it.
    #[must_use]
    pub fn further_letters_of(&self, letter: char) -> Option<&'static str> {
        (self.spec.further_letters.iter())
            .find(|(base, _)| *base == letter)
            .map(|(_, letters)| *letters)
    }
}

pub static ENGLISH: Layout = Layout::shipped(LayoutSpec {
    id: "en-qwerty",
    language: language!("en"),
    name: "English",
    arrangement: "QWERTY",
    rows: ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
    further_letters: &[
        ('a', "àáâäå"),
        ('c', "ç"),
        ('e', "éèêë"),
        ('i', "íïî"),
        ('n', "ñ"),
        ('o', "óöôø"),
        ('u', "úüû"),
    ],
    currency: "$",
});

pub static GERMAN: Layout = Layout::shipped(LayoutSpec {
    id: "de-qwertz",
    language: language!("de"),
    name: "Deutsch",
    arrangement: "QWERTZ",
    rows: ["qwertzuiopü", "asdfghjklöä", "yxcvbnm"],
    further_letters: &[
        ('a', "äàâ"),
        ('c', "ç"),
        ('e', "éèê"),
        ('o', "ö"),
        ('s', "ß"),
        ('u', "ü"),
    ],
    currency: "€",
});

pub static FRENCH: Layout = Layout::shipped(LayoutSpec {
    id: "fr-azerty",
    language: language!("fr"),
    name: "Français",
    arrangement: "AZERTY",
    rows: ["azertyuiop", "qsdfghjklm", "wxcvbn'"],
    further_letters: &[
        ('a', "àâæ"),
        ('c', "ç"),
        ('e', "éèêë"),
        ('i', "îï"),
        ('o', "ôœ"),
        ('u', "ûùü"),
        ('y', "ÿ"),
    ],
    currency: "€",
});

pub static PORTUGUESE: Layout = Layout::shipped(LayoutSpec {
    id: "pt-qwerty",
    language: language!("pt"),
    name: "Português",
    arrangement: "QWERTY",
    rows: ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
    further_letters: &[
        ('a', "ãâáà"),
        ('c', "ç"),
        ('e', "êé"),
        ('i', "í"),
        ('o', "ôóõ"),
        ('u', "úü"),
    ],
    currency: "€",
});

pub static LAYOUTS: [&Layout; 4] = [&ENGLISH, &GERMAN, &FRENCH, &PORTUGUESE];

/// The layouts among `candidates` whose language is the language of one of `locales`, each once.
/// They come in the order of `locales` and, for one locale, in the order of `candidates`. When no
/// candidate matches a locale, or `locales` is empty, all `candidates` are returned in their order.
pub(crate) fn offered(
    candidates: &[&'static Layout],
    locales: &[LanguageIdentifier],
) -> Vec<&'static Layout> {
    let mut offered: Vec<&'static Layout> = Vec::new();
    for locale in locales {
        for layout in candidates
            .iter()
            .copied()
            .filter(|layout| layout.written_in(locale))
        {
            if !offered.iter().any(|listed| listed.id() == layout.id()) {
                offered.push(layout);
            }
        }
    }
    if offered.is_empty() {
        candidates.to_vec()
    } else {
        offered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icu_locale_core::langid;

    #[test]
    fn each_layout_is_found_by_its_id_and_offers_further_letters_on_keys_of_the_layout() {
        for layout in LAYOUTS {
            assert_eq!(Layout::find(layout.id()), Some(layout));
            let rows = layout.rows();
            let mut letters: Vec<char> = rows.iter().flat_map(|row| row.chars()).collect();
            let count = letters.len();
            letters.sort_unstable();
            letters.dedup();
            assert_eq!(letters.len(), count, "{} has each letter once", layout.id());
            assert_eq!(Layout::new(layout.spec).as_ref(), Ok(layout));
            for (base, further) in layout.spec.further_letters {
                assert!(
                    letters.contains(base),
                    "{} has a key for {base}",
                    layout.id()
                );
                assert!(
                    !further.contains(*base),
                    "{base} is typed by its key itself"
                );
                let mut listed: Vec<char> = further.chars().collect();
                listed.sort_unstable();
                listed.dedup();
                assert_eq!(
                    listed.len(),
                    further.chars().count(),
                    "{base} lists each once"
                );
                assert_eq!(further.to_lowercase(), *further, "{base} lists lower case");
                assert_eq!(layout.further_letters_of(*base), Some(*further));
            }
        }
        assert_eq!(Layout::find("xx-unknown"), None);
        assert_eq!(ENGLISH.further_letters_of('q'), None);
    }

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
            currency: "$",
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

    #[test]
    fn a_locale_asks_for_the_layouts_of_its_primary_language_in_the_order_of_the_locales() {
        let ids =
            |layouts: Vec<&Layout>| -> Vec<&str> { layouts.into_iter().map(Layout::id).collect() };
        assert_eq!(
            ids(offered(
                &LAYOUTS,
                &[
                    langid!("de-AT"),
                    langid!("en-US"),
                    langid!("DE"),
                    langid!("fr")
                ]
            )),
            ["de-qwertz", "en-qwerty", "fr-azerty"],
            "a language comes once, at its first locale"
        );
        assert_eq!(
            ids(offered(
                &[&PORTUGUESE, &ENGLISH],
                &[langid!("en"), langid!("pt-BR")]
            )),
            ["en-qwerty", "pt-qwerty"]
        );
    }

    #[test]
    fn languages_without_a_layout_or_none_offer_every_candidate_in_its_order() {
        let ids =
            |layouts: Vec<&Layout>| -> Vec<&str> { layouts.into_iter().map(Layout::id).collect() };
        let all = ["de-qwertz", "en-qwerty"];
        assert_eq!(ids(offered(&[&GERMAN, &ENGLISH], &[langid!("ja-JP")])), all);
        assert_eq!(ids(offered(&[&GERMAN, &ENGLISH], &[])), all);
        assert!(offered(&[], &[langid!("ja-JP")]).is_empty());
    }
}
