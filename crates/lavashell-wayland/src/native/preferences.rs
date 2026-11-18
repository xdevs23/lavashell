// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use crate::translator::ViewLink;
use icu_locale_core::LanguageIdentifier;
use lavashell_desktop_settings::SettingsSource;
use lavashell_platform::SettingsEvent;
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};

mod error;

pub use error::PreferencesError;

fn user_languages() -> Vec<LanguageIdentifier> {
    languages(sys_locale::get_locales())
}

fn languages(tags: impl Iterator<Item = String>) -> Vec<LanguageIdentifier> {
    tags.filter_map(|tag| tag.parse().ok()).collect()
}

pub(crate) fn install<Data>(
    handle: &LoopHandle<'static, Data>,
    view: &ViewLink,
    settings: Vec<Box<dyn SettingsSource>>,
) -> Result<RegistrationToken, PreferencesError> {
    view.report(SettingsEvent::Locales(user_languages()));
    let (executor, scheduler) =
        calloop::futures::executor::<()>().map_err(PreferencesError::CreatePreferenceWatch)?;
    let watch = handle
        .insert_source(executor, |(), (), _| {})
        .map_err(|failure| PreferencesError::WatchPreferences(failure.error))?;
    for source in settings {
        let view = view.clone();
        scheduler
            .schedule(source.watch(Box::new(move |event| view.report(event))))
            .map_err(PreferencesError::SchedulePreferenceWatch)?;
    }
    Ok(watch)
}

#[cfg(test)]
mod tests;
