// SPDX-License-Identifier: GPL-3.0-only

use crate::geometry::Color;
use crate::local::Local;
use crate::units::{Dp, Sp};
use icu_locale_core::LanguageIdentifier;
use lavashell_enum_values::enum_with_values;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformFacts {
    pub slop: Option<Dp>,
    pub caret_width: Option<Dp>,
    pub long_press: Duration,
    pub double_press: Duration,
    pub key_repeat: Option<KeyRepeat>,
    pub text_size: Sp,
    pub foreground: Color,
    pub color_scheme: ColorScheme,
    pub password_mask: char,
    pub keyboard: KeyboardPreferences,
    pub locales: Vec<LanguageIdentifier>,
}

const DEFAULT_LONG_PRESS: Duration = Duration::from_millis(300);
const DEFAULT_DOUBLE_PRESS: Duration = Duration::from_millis(300);
const DEFAULT_KEY_REPEAT_DELAY: Duration = Duration::from_millis(400);
const DEFAULT_KEY_REPEAT_INTERVAL: Duration = Duration::from_millis(50);
const DEFAULT_TEXT_SIZE: Sp = Sp::new(14.0);

impl Default for PlatformFacts {
    fn default() -> Self {
        Self {
            slop: None,
            caret_width: None,
            long_press: DEFAULT_LONG_PRESS,
            double_press: DEFAULT_DOUBLE_PRESS,
            key_repeat: KeyRepeat::new(DEFAULT_KEY_REPEAT_DELAY, DEFAULT_KEY_REPEAT_INTERVAL),
            text_size: DEFAULT_TEXT_SIZE,
            foreground: Color::WHITE,
            color_scheme: ColorScheme::Dark,
            password_mask: '•',
            keyboard: KeyboardPreferences {
                automatic_capitals: true,
                key_preview: true,
            },
            locales: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyRepeat {
    delay: Duration,
    interval: Duration,
}

impl KeyRepeat {
    #[must_use]
    pub fn new(delay: Duration, interval: Duration) -> Option<Self> {
        (!interval.is_zero()).then_some(Self { delay, interval })
    }

    #[must_use]
    pub fn delay(self) -> Duration {
        self.delay
    }

    #[must_use]
    pub fn interval(self) -> Duration {
        self.interval
    }
}

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ColorScheme {
        Light,
        Dark,
    }

    pub const ALL;
}

impl ColorScheme {
    #[must_use]
    pub fn select<T>(self, dark: T, light: T) -> T {
        match self {
            Self::Dark => dark,
            Self::Light => light,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardPreferences {
    pub automatic_capitals: bool,
    pub key_preview: bool,
}

pub static SLOP: Local<Option<Dp>> = Local::platform(|platform| platform.slop);
pub static CARET_WIDTH: Local<Option<Dp>> = Local::platform(|platform| platform.caret_width);
pub static LONG_PRESS: Local<Duration> = Local::platform(|platform| platform.long_press);
pub static DOUBLE_PRESS: Local<Duration> = Local::platform(|platform| platform.double_press);
pub static KEY_REPEAT: Local<Option<KeyRepeat>> = Local::platform(|platform| platform.key_repeat);
pub static COLOR_SCHEME: Local<ColorScheme> = Local::platform(|platform| platform.color_scheme);
pub static PASSWORD_MASK: Local<char> = Local::platform(|platform| platform.password_mask);
pub static KEYBOARD_PREFERENCES: Local<KeyboardPreferences> =
    Local::platform(|platform| platform.keyboard);
pub static LOCALES: Local<Vec<LanguageIdentifier>> =
    Local::platform(|platform| platform.locales.clone());

#[cfg(any(test, feature = "test-support"))]
pub mod fixture;
