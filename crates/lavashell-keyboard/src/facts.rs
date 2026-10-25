// SPDX-License-Identifier: GPL-3.0-only
use icu_locale_core::LanguageIdentifier;
use lavashell_core::platform::{
    DOUBLE_TAP, KEY_REPEAT, KEYBOARD_PREFERENCES, KeyRepeat, KeyboardPreferences, LOCALES,
    LONG_PRESS,
};
use lavashell_core::prelude::Ui;
use std::time::Duration;

/// The platform facts used by the keyboard: the long press and double tap durations, the key repeat
/// timing, the keyboard preferences and the locales. The keyboard reads them during each
/// composition, so a press after that composition uses those values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Facts {
    pub long_press: Duration,
    pub double_tap: Duration,
    pub key_repeat: Option<KeyRepeat>,
    pub preferences: KeyboardPreferences,
    pub locales: Vec<LanguageIdentifier>,
}

impl Facts {
    pub fn read(ui: &Ui) -> Self {
        Self {
            long_press: ui.local(&LONG_PRESS),
            double_tap: ui.local(&DOUBLE_TAP),
            key_repeat: ui.local(&KEY_REPEAT),
            preferences: ui.local(&KEYBOARD_PREFERENCES),
            locales: ui.local(&LOCALES),
        }
    }
}
