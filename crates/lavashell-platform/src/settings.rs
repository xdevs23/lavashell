// SPDX-License-Identifier: GPL-3.0-only
use icu_locale_core::LanguageIdentifier;
use lavashell_core::platform::KeyRepeat;
use lavashell_core::preferences::Appearance;

/// A setting of the host system. Each event replaces the last event of its kind.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsEvent {
    Appearance(Appearance),
    /// How a held key repeats, or `None` where a held key does not repeat.
    KeyRepeat(Option<KeyRepeat>),
    /// The languages the user prefers, most preferred first. An empty list leaves the languages supplied by the app
    /// in place.
    Locales(Vec<LanguageIdentifier>),
}
