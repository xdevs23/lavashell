// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use icu_locale_core::langid;

#[test]
fn locale_tags_that_are_not_a_language_are_dropped() {
    let tags = ["de-DE", "C", "en"].map(str::to_owned);
    assert_eq!(
        languages(tags.into_iter()),
        [langid!("de-DE"), langid!("en")]
    );
    assert!(languages(["C".to_owned()].into_iter()).is_empty());
}
