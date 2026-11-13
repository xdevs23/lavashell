// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::Error;
use crate::test_error::TestError;
use icu_locale_core::langid;
use lavashell_actor::mailbox;
use lavashell_platform::PlatformEvent;
use sctk::reexports::calloop::EventLoop;

#[test]
fn locale_tags_that_are_not_a_language_are_dropped() {
    let tags = ["de-DE", "C", "en"].map(str::to_owned);
    assert_eq!(
        languages(tags.into_iter()),
        [langid!("de-DE"), langid!("en")]
    );
    assert!(languages(["C".to_owned()].into_iter()).is_empty());
}

#[test]
fn the_languages_of_the_user_reach_the_view_before_the_desktop_reports_a_preference()
-> Result<(), TestError> {
    let event_loop: EventLoop<'static, Translator> = EventLoop::try_new()?;
    let (sender, view) = mailbox::<PlatformEvent>();
    let mut sources = Vec::new();
    let link = ViewLink::new(sender.outlet(|event| event));
    install(&event_loop.handle(), &mut sources, &link).map_err(Error::from)?;
    assert!(matches!(
        view.take_arrived()?[..],
        [PlatformEvent::Settings(SettingsEvent::Locales(_))]
    ));
    Ok(())
}
