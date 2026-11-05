// SPDX-License-Identifier: GPL-3.0-only
use super::{LoopHandle, Platform, sctk};
use crate::Error;
use icu_locale_core::LanguageIdentifier;
use lavashell_platform::SettingsEvent;
use sctk::reexports::calloop::RegistrationToken;
use sctk::reexports::calloop::channel::{self, Event};

/// The languages the user prefers, read from the locale variables of the environment, most
/// preferred first.
pub(super) fn user_languages() -> Vec<LanguageIdentifier> {
    languages(sys_locale::get_locales())
}

/// The language identifiers among the BCP 47 `tags`. A tag that is not a language identifier,
/// such as the POSIX locale `C`, is dropped.
fn languages(tags: impl Iterator<Item = String>) -> Vec<LanguageIdentifier> {
    tags.filter_map(|tag| tag.parse().ok()).collect()
}

/// Watches the appearance preferences of the desktop on the event loop of `handle`. Removing the
/// `sources` that it adds ends the watch.
pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<(), Error> {
    let (sender, channel) = channel::channel();
    let updates = handle
        .insert_source(channel, |event, (), platform| {
            if let Event::Msg(appearance) = event {
                let changed = SettingsEvent::Appearance(appearance);
                platform.surface_records.change_settings(changed);
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
