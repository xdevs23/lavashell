// SPDX-License-Identifier: GPL-3.0-only
use icu_locale_core::LanguageIdentifier;
use lavashell_core::platform::{ColorScheme, KeyRepeat, PlatformFacts};
use lavashell_core::preferences::{PreferenceOverrides, PreferenceSource};
use lavashell_core::view::View;
use lavashell_platform::SettingsEvent;

pub(crate) struct HostSettings {
    preferences: PreferenceSource,
    facts: HostFacts,
}

#[derive(Default)]
struct HostFacts {
    key_repeat: HostRepeat,
    color_scheme: Option<ColorScheme>,
    locales: Vec<LanguageIdentifier>,
}

#[derive(Clone, Copy, Default)]
enum HostRepeat {
    #[default]
    Unreported,
    Reported(Option<KeyRepeat>),
}

impl HostSettings {
    pub fn new(overrides: PreferenceOverrides) -> Self {
        let mut preferences = PreferenceSource::default();
        preferences.set_overrides(overrides);
        Self {
            preferences,
            facts: HostFacts::default(),
        }
    }

    pub fn change(&mut self, event: SettingsEvent) {
        match event {
            SettingsEvent::Appearance(appearance) => {
                let system = appearance.applied_to(self.preferences.system());
                self.preferences.set_system(system);
                self.facts.color_scheme = appearance.color_scheme;
            }
            SettingsEvent::KeyRepeat(repeat) => {
                self.facts.key_repeat = HostRepeat::Reported(repeat);
            }
            SettingsEvent::Locales(locales) => self.facts.locales = locales,
        }
    }

    pub fn provide<'a>(&self, views: impl IntoIterator<Item = (&'a View, &'a PlatformFacts)>) {
        let values = self.preferences.values();
        for (view, supplied) in views {
            view.provide_preferences(&values);
            view.provide_platform(self.facts.applied_to(supplied));
        }
    }
}

impl HostFacts {
    fn applied_to(&self, supplied: &PlatformFacts) -> PlatformFacts {
        let mut facts = supplied.clone();
        if let HostRepeat::Reported(repeat) = self.key_repeat {
            facts.key_repeat = repeat;
        }
        if let Some(scheme) = self.color_scheme {
            facts.color_scheme = scheme;
        }
        if !self.locales.is_empty() {
            facts.locales.clone_from(&self.locales);
        }
        facts
    }
}

#[cfg(test)]
mod tests;
