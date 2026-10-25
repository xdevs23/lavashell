// SPDX-License-Identifier: GPL-3.0-only
//! Fixed platform facts for the tests of the other crates, which run their views with these facts in place of the
//! facts of a real platform.
#![forbid(unsafe_code)]

use icu_locale_core::langid;
use lavashell_core::geometry::Color;
use lavashell_core::platform::{ColorScheme, KeyRepeat, KeyboardPreferences, PlatformFacts};
use lavashell_core::units::{DpExt, SpExt};
use std::time::Duration;

/// How long a press lasts before it is a long press, in milliseconds.
pub const LONG_PRESS: u64 = 300;
/// The longest time between two taps of a double tap, in milliseconds.
pub const DOUBLE_TAP: u64 = 300;
/// The delay before a held key starts to repeat, in milliseconds.
pub const REPEAT_DELAY: u64 = 400;
/// The time between two repeats of a held key, in milliseconds.
pub const REPEAT_INTERVAL: u64 = 50;

/// The platform facts with which the tests run their views. The timing values are the constants of this crate.
#[must_use]
pub fn facts() -> PlatformFacts {
    PlatformFacts {
        touch_slop: Some(8.dp()),
        caret_width: Some(1.dp()),
        long_press: Duration::from_millis(LONG_PRESS),
        double_tap: Duration::from_millis(DOUBLE_TAP),
        key_repeat: KeyRepeat::new(
            Duration::from_millis(REPEAT_DELAY),
            Duration::from_millis(REPEAT_INTERVAL),
        ),
        text_size: 14.sp(),
        foreground: Color::WHITE,
        color_scheme: ColorScheme::Dark,
        password_mask: '•',
        keyboard: KeyboardPreferences {
            automatic_capitals: true,
            key_preview: true,
        },
        locales: vec![langid!("en")],
    }
}
