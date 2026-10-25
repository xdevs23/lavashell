// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::units::{DpExt, SpExt};
use icu_locale_core::langid;

/// The platform facts with which the tests of this crate create their views.
pub(crate) fn facts() -> PlatformFacts {
    PlatformFacts {
        touch_slop: Some(8.dp()),
        caret_width: Some(1.dp()),
        long_press: Duration::from_millis(300),
        double_tap: Duration::from_millis(300),
        key_repeat: KeyRepeat::new(Duration::from_millis(400), Duration::from_millis(50)),
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
