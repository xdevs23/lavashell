// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::translator::{Translator, ViewLink};
use icu_locale_core::LanguageIdentifier;
use lavashell_platform::SettingsEvent;
use sctk::reexports::calloop::channel::{self, Event};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;

/// The languages the user prefers, read from the locale variables of the environment, most
/// preferred first.
fn user_languages() -> Vec<LanguageIdentifier> {
    languages(sys_locale::get_locales())
}

/// The language identifiers among the BCP 47 `tags`. A tag that is not a language identifier,
/// such as the POSIX locale `C`, is dropped.
fn languages(tags: impl Iterator<Item = String>) -> Vec<LanguageIdentifier> {
    tags.filter_map(|tag| tag.parse().ok()).collect()
}

/// Reports the languages the user prefers to `view`, and watches the appearance preferences of
/// the desktop on the event loop of `handle`. Removing the `sources` that it adds ends the watch.
pub(crate) fn install(
    handle: &LoopHandle<'static, Translator>,
    sources: &mut Vec<RegistrationToken>,
    view: &ViewLink,
) -> Result<(), Error> {
    view.report(SettingsEvent::Locales(user_languages()));
    let (sender, channel) = channel::channel();
    let updates = handle
        .insert_source(channel, |event, (), translator| {
            if let Event::Msg(appearance) = event {
                translator
                    .links
                    .report(SettingsEvent::Appearance(appearance));
            }
        })
        .map_err(|failure| Error::WatchPreferences(failure.error))?;
    sources.push(updates);
    let (executor, scheduler) =
        calloop::futures::executor::<zbus::Result<()>>().map_err(Error::CreatePreferenceWatch)?;
    let watch = handle
        .insert_source(executor, |watched, (), _| {
            if let Err(error) = watched {
                log::info!("the appearance preferences of the desktop are unavailable: {error}");
            }
        })
        .map_err(|failure| Error::WatchPreferences(failure.error))?;
    sources.push(watch);
    scheduler
        .schedule(crate::preferences::watch(sender))
        .map_err(Error::SchedulePreferenceWatch)
}

#[cfg(test)]
mod tests;
