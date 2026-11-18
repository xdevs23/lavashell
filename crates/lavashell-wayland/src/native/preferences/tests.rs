// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::Error;
use crate::test_error::TestError;
use icu_locale_core::langid;
use lavashell_actor::mailbox;
use lavashell_desktop_settings::{SettingsReporter, SettingsWatch};
use lavashell_platform::PlatformEvent;
use sctk::reexports::calloop::EventLoop;
use std::time::Duration;

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
fn the_languages_of_the_user_reach_the_view_before_the_desktop_reports_a_preference()
-> Result<(), TestError> {
    let event_loop: EventLoop<'static, ()> = EventLoop::try_new()?;
    let (sender, view) = mailbox::<PlatformEvent>();
    let link = ViewLink::new(sender.outlet(|event| event));
    let _registrations_stay_installed_for_the_test =
        install(&event_loop.handle(), &link, Vec::new()).map_err(Error::from)?;
    assert!(matches!(
        view.take_arrived()?[..],
        [PlatformEvent::Settings(SettingsEvent::Locales(_))]
    ));
    Ok(())
}

struct EndsAtOnce;

impl SettingsSource for EndsAtOnce {
    fn watch(self: Box<Self>, _report: SettingsReporter) -> SettingsWatch {
        Box::pin(async {})
    }
}

struct ReportsKeyRepeat;

impl SettingsSource for ReportsKeyRepeat {
    fn watch(self: Box<Self>, report: SettingsReporter) -> SettingsWatch {
        Box::pin(async move { report(SettingsEvent::KeyRepeat(None)) })
    }
}

#[test]
fn a_source_reports_to_the_view_after_another_source_ended() -> Result<(), TestError> {
    let mut event_loop: EventLoop<'static, ()> = EventLoop::try_new()?;
    let (sender, view) = mailbox::<PlatformEvent>();
    let link = ViewLink::new(sender.outlet(|event| event));
    let sources: Vec<Box<dyn SettingsSource>> =
        vec![Box::new(EndsAtOnce), Box::new(ReportsKeyRepeat)];
    let _registration_stays_installed_for_the_test =
        install(&event_loop.handle(), &link, sources).map_err(Error::from)?;
    event_loop.dispatch(Duration::ZERO, &mut ())?;
    assert!(matches!(
        view.take_arrived()?[..],
        [
            PlatformEvent::Settings(SettingsEvent::Locales(_)),
            PlatformEvent::Settings(SettingsEvent::KeyRepeat(None)),
        ]
    ));
    Ok(())
}
