// SPDX-License-Identifier: GPL-3.0-only
use super::layout::Layout;
use icu_locale_core::LanguageIdentifier;

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
            if !offered.contains(&layout) {
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
    use crate::layouts::{LayoutError, ShippedLayouts};
    use icu_locale_core::langid;

    #[test]
    fn a_locale_asks_for_the_layouts_of_its_primary_language_in_the_order_of_the_locales()
    -> Result<(), LayoutError> {
        let shipped = ShippedLayouts::load()?;
        let ids =
            |layouts: Vec<&Layout>| -> Vec<&str> { layouts.into_iter().map(Layout::id).collect() };
        assert_eq!(
            ids(offered(
                &shipped.all(),
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
                &[&shipped.portuguese, &shipped.english],
                &[langid!("en"), langid!("pt-BR")]
            )),
            ["en-qwerty", "pt-qwerty"]
        );
        Ok(())
    }

    #[test]
    fn languages_without_a_layout_or_none_offer_every_candidate_in_its_order()
    -> Result<(), LayoutError> {
        let shipped = ShippedLayouts::load()?;
        let candidates = [&shipped.german, &shipped.english];
        let ids =
            |layouts: Vec<&Layout>| -> Vec<&str> { layouts.into_iter().map(Layout::id).collect() };
        let all = ["de-qwertz", "en-qwerty"];
        assert_eq!(ids(offered(&candidates, &[langid!("ja-JP")])), all);
        assert_eq!(ids(offered(&candidates, &[])), all);
        assert!(offered(&[], &[langid!("ja-JP")]).is_empty());
        Ok(())
    }
}
