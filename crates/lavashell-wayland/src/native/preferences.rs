// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use crate::translator::{Translator, ViewLink};
use icu_locale_core::LanguageIdentifier;
use lavashell_platform::SettingsEvent;
use sctk::reexports::calloop::channel::{self, Event};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};

mod error;

pub use error::PreferencesError;

fn user_languages() -> Vec<LanguageIdentifier> {
    languages(sys_locale::get_locales())
}

fn languages(tags: impl Iterator<Item = String>) -> Vec<LanguageIdentifier> {
    tags.filter_map(|tag| tag.parse().ok()).collect()
}

pub(crate) fn install(
    handle: &LoopHandle<'static, Translator>,
    view: &ViewLink,
) -> Result<[RegistrationToken; 2], PreferencesError> {
    view.report(SettingsEvent::Locales(user_languages()));
    let (sender, channel) = channel::channel();
    let appearance_view = view.clone();
    let updates = handle
        .insert_source(channel, move |event, (), _| {
            if let Event::Msg(appearance) = event {
                appearance_view.report(SettingsEvent::Appearance(appearance));
            }
        })
        .map_err(|failure| PreferencesError::WatchPreferences(failure.error))?;
    let (executor, scheduler) = calloop::futures::executor::<zbus::Result<()>>()
        .map_err(PreferencesError::CreatePreferenceWatch)?;
    let watch = handle
        .insert_source(executor, |watched, (), _| {
            if let Err(error) = watched {
                log::info!("the appearance preferences of the desktop are unavailable: {error}");
            }
        })
        .map_err(|failure| PreferencesError::WatchPreferences(failure.error))?;
    scheduler
        .schedule(crate::preferences::watch(sender))
        .map_err(PreferencesError::SchedulePreferenceWatch)?;
    Ok([updates, watch])
}

#[cfg(test)]
mod tests;
