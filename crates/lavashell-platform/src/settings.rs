// SPDX-License-Identifier: GPL-3.0-only
use icu_locale_core::LanguageIdentifier;
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::platform::KeyRepeat;
use lavashell_core::preferences::Appearance;
use std::mem::{self, Discriminant};

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsEvent {
    Appearance(Appearance),
    KeyRepeat(Option<KeyRepeat>),
    Locales(Vec<LanguageIdentifier>),
}

impl Merge for SettingsEvent {
    type Key = Discriminant<Self>;

    fn effect(&self) -> MergeEffect<Discriminant<Self>> {
        MergeEffect::Overwrites(mem::discriminant(self))
    }
}
