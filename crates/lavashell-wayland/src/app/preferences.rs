// SPDX-License-Identifier: GPL-3.0-only
use super::{Instant, LoopHandle, Platform, PlatformFacts, View, sctk};
use crate::Error;
use crate::preferences::Appearance;
use icu_locale_core::LanguageIdentifier;
use lavashell_core::platform::{ColorScheme, KeyRepeat};
use lavashell_core::preferences::PreferenceSource;
use sctk::reexports::calloop::RegistrationToken;
use sctk::reexports::calloop::channel::{self, Event};

/// The key repeat that a keyboard reported last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum HostRepeat {
    /// No keyboard has reported its repeat settings yet, so the key repeat supplied by the app applies.
    #[default]
    Unreported,
    Disabled,
    Repeats(KeyRepeat),
}

/// The platform facts reported by the compositor, the desktop and the environment. Each fact known
/// here replaces the one supplied by the app.
#[derive(Clone, Default)]
pub(super) struct HostFacts {
    pub key_repeat: HostRepeat,
    /// `None` while the desktop reports no preferred color scheme.
    pub color_scheme: Option<ColorScheme>,
    /// `None` when the locale variables of the environment contain no language.
    pub locales: Option<Vec<LanguageIdentifier>>,
}

impl HostFacts {
    /// Provides `view` with the facts in `supplied`, after replacing each fact that the host knows with the value
    /// from the host. Skips the call when the view already has those facts.
    fn apply(&self, view: &View, supplied: &PlatformFacts) {
        let mut facts = supplied.clone();
        match self.key_repeat {
            HostRepeat::Unreported => {}
            HostRepeat::Disabled => facts.key_repeat = None,
            HostRepeat::Repeats(repeat) => facts.key_repeat = Some(repeat),
        }
        if let Some(scheme) = self.color_scheme {
            facts.color_scheme = scheme;
        }
        if let Some(locales) = &self.locales {
            facts.locales.clone_from(locales);
        }
        if facts != view.platform() {
            view.provide_platform(facts);
        }
    }
}

/// The languages of the user, read from the locale variables of the environment, the most preferred
/// first, or `None` when the variables contain no language.
pub(super) fn user_languages() -> Option<Vec<LanguageIdentifier>> {
    languages(sys_locale::get_locales())
}

/// The language identifiers among the BCP 47 `tags`. A tag that is not a language identifier, such
/// as the POSIX locale `C`, is left out.
fn languages(tags: impl Iterator<Item = String>) -> Option<Vec<LanguageIdentifier>> {
    let languages: Vec<_> = tags
        .filter_map(|tag| {
            (tag.parse::<LanguageIdentifier>())
                .inspect_err(|error| log::debug!("the locale {tag} names no language: {error}"))
                .ok()
        })
        .collect();
    (!languages.is_empty()).then_some(languages)
}

/// The view of a surface and the facts that the app supplied for it.
pub(super) type SuppliedView<'a> = (&'a View, &'a PlatformFacts);

pub(super) struct ViewConfiguration {
    pub preferences: PreferenceSource,
    pub facts: HostFacts,
}

impl ViewConfiguration {
    /// Sets up a new view: its animations use the time elapsed since `start`, and it gets the current
    /// preferences and host facts. Returns the facts that the app supplied for the view.
    pub fn apply(&self, view: &View, start: Instant) -> PlatformFacts {
        view.motion().time_source(move || start.elapsed());
        view.provide_preferences(&self.preferences.values());
        let supplied = view.platform();
        self.facts.apply(view, &supplied);
        supplied
    }

    pub fn update<'a>(
        &mut self,
        appearance: Appearance,
        views: impl Iterator<Item = SuppliedView<'a>>,
    ) {
        let system = appearance.applied_to(self.preferences.system());
        self.preferences.set_system(system);
        self.facts.color_scheme = appearance.color_scheme;
        let values = self.preferences.values();
        views.for_each(|(view, supplied)| {
            view.provide_preferences(&values);
            self.facts.apply(view, supplied);
        });
    }

    pub fn repeat<'a>(
        &mut self,
        repeat: HostRepeat,
        views: impl Iterator<Item = SuppliedView<'a>>,
    ) {
        self.facts.key_repeat = repeat;
        views.for_each(|(view, supplied)| self.facts.apply(view, supplied));
    }
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
                let views = (platform.surfaces.iter()).filter_map(|(&id, surface)| {
                    let view = platform.surface_records.open_view(id)?;
                    Some((view, &surface.supplied_facts))
                });
                platform.view_configuration.update(appearance, views);
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
