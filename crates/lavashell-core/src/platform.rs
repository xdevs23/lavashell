// SPDX-License-Identifier: GPL-3.0-only
//! Facts about the platform on which a view runs, such as the touch slop, the text size and the languages of the
//! user. The host supplies them when it creates the view. The host reads each fact from the platform where the
//! platform has a source for it, and gets it from the application otherwise. Each fact is passed down the view tree
//! under a separate key.

use crate::geometry::Color;
use crate::local::Local;
use crate::units::{Dp, Sp};
use icu_locale_core::LanguageIdentifier;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct PlatformFacts {
    /// How far a pressed pointer must move before the move counts as a drag, a scroll or a pinch. When the platform
    /// has no value, a move by one device pixel counts.
    pub touch_slop: Option<Dp>,
    /// The width of the text caret. When the platform has no value, the caret is one device pixel wide.
    pub caret_width: Option<Dp>,
    /// How long a press is held before it counts as a long press.
    pub long_press: Duration,
    /// The longest time between the taps of a double tap.
    pub double_tap: Duration,
    /// How a held key repeats, or None where a held key does not repeat.
    pub key_repeat: Option<KeyRepeat>,
    /// The size of body text.
    pub text_size: Sp,
    /// The color of text and icons.
    pub foreground: Color,
    pub color_scheme: ColorScheme,
    /// The character shown in place of each character of hidden text.
    pub password_mask: char,
    pub keyboard: KeyboardPreferences,
    /// The languages of the user, the most preferred first.
    pub locales: Vec<LanguageIdentifier>,
}

/// How a held key repeats: first after `delay`, then once every `interval`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyRepeat {
    delay: Duration,
    interval: Duration,
}

impl KeyRepeat {
    /// None for an interval of zero, which would repeat in every frame.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// The preferences of the user for on-screen keyboards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardPreferences {
    /// Whether the first letter of a sentence is a capital.
    pub automatic_capitals: bool,
    /// Whether a pressed key shows its letter above the finger.
    pub key_preview: bool,
}

pub static TOUCH_SLOP: Local<Option<Dp>> = Local::platform(|platform| platform.touch_slop);
pub static CARET_WIDTH: Local<Option<Dp>> = Local::platform(|platform| platform.caret_width);
pub static LONG_PRESS: Local<Duration> = Local::platform(|platform| platform.long_press);
pub static DOUBLE_TAP: Local<Duration> = Local::platform(|platform| platform.double_tap);
pub static KEY_REPEAT: Local<Option<KeyRepeat>> = Local::platform(|platform| platform.key_repeat);
pub static COLOR_SCHEME: Local<ColorScheme> = Local::platform(|platform| platform.color_scheme);
pub static PASSWORD_MASK: Local<char> = Local::platform(|platform| platform.password_mask);
pub static KEYBOARD_PREFERENCES: Local<KeyboardPreferences> =
    Local::platform(|platform| platform.keyboard);
pub static LOCALES: Local<Vec<LanguageIdentifier>> =
    Local::platform(|platform| platform.locales.clone());

#[cfg(test)]
pub(crate) mod fixture;
