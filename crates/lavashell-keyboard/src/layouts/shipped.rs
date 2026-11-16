// SPDX-License-Identifier: GPL-3.0-only
use super::currency::Currency;
use super::layout::{Layout, LayoutError, LayoutSpec};
use icu_locale_core::subtags::language;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct ShippedLayouts {
    pub english: Layout,
    pub german: Layout,
    pub french: Layout,
    pub portuguese: Layout,
}

static SHIPPED: OnceLock<Result<ShippedLayouts, LayoutError>> = OnceLock::new();

impl ShippedLayouts {
    pub fn load() -> Result<&'static Self, LayoutError> {
        (SHIPPED.get_or_init(Self::build))
            .as_ref()
            .map_err(|error| *error)
    }

    fn build() -> Result<Self, LayoutError> {
        Ok(Self {
            english: Layout::new(ENGLISH)?,
            german: Layout::new(GERMAN)?,
            french: Layout::new(FRENCH)?,
            portuguese: Layout::new(PORTUGUESE)?,
        })
    }

    #[must_use]
    pub fn all(&self) -> [&Layout; 4] {
        [&self.english, &self.german, &self.french, &self.portuguese]
    }

    #[must_use]
    pub fn find(&self, id: &str) -> Option<&Layout> {
        self.all().into_iter().find(|layout| layout.id() == id)
    }
}

const ENGLISH: LayoutSpec = LayoutSpec {
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
    currency: Currency::DOLLAR,
};

const GERMAN: LayoutSpec = LayoutSpec {
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
    currency: Currency::EURO,
};

const FRENCH: LayoutSpec = LayoutSpec {
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
    currency: Currency::EURO,
};

const PORTUGUESE: LayoutSpec = LayoutSpec {
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
    currency: Currency::EURO,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_layouts_are_built_once() -> Result<(), LayoutError> {
        assert!(std::ptr::eq(
            ShippedLayouts::load()?,
            ShippedLayouts::load()?
        ));
        Ok(())
    }

    #[test]
    fn no_two_layouts_share_an_id() -> Result<(), LayoutError> {
        let layouts = ShippedLayouts::load()?.all();
        for (position, layout) in layouts.iter().enumerate() {
            assert!(
                layouts[position + 1..].iter().all(|other| layout != other),
                "{} is shipped once",
                layout.id()
            );
        }
        Ok(())
    }

    #[test]
    fn each_layout_is_found_by_its_id_and_offers_further_letters_on_keys_of_the_layout()
    -> Result<(), LayoutError> {
        let shipped = ShippedLayouts::load()?;
        for layout in shipped.all() {
            assert_eq!(shipped.find(layout.id()), Some(layout));
            let rows = layout.rows();
            let mut letters: Vec<char> = rows.iter().flat_map(|row| row.chars()).collect();
            let count = letters.len();
            letters.sort_unstable();
            letters.dedup();
            assert_eq!(letters.len(), count, "{} has each letter once", layout.id());
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
        assert_eq!(shipped.find("xx-unknown"), None);
        assert_eq!(shipped.english.further_letters_of('q'), None);
        Ok(())
    }
}
