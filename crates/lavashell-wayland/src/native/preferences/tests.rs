// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use icu_locale_core::langid;
use lavashell_actor::mailbox;
use lavashell_platform::PlatformEvent;

#[test]
fn locale_tags_that_are_not_a_language_are_dropped() {
    let tags = ["de-DE", "C", "en"].map(str::to_owned);
    assert_eq!(
        languages(tags.into_iter()),
        [langid!("de-DE"), langid!("en")]
    );
    assert_eq!(languages(["C".to_owned()].into_iter()), []);
}

#[test]
fn the_languages_of_the_user_reach_the_view() -> Result<(), TestError> {
    let (sender, view) = mailbox::<PlatformEvent>();
    report_user_languages(&ViewLink::new(sender.outlet(|event| event)));
    assert!(matches!(
        view.take_arrived()?[..],
        [PlatformEvent::Settings(SettingsEvent::Locales(_))]
    ));
    Ok(())
}
