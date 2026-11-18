// SPDX-License-Identifier: GPL-3.0-only
use super::{ColorScheme, KeyRepeat, KeyboardPreferences, PlatformFacts};
use crate::geometry::Color;
use crate::units::{DpExt, SpExt};
use icu_locale_core::langid;
use std::time::Duration;

pub const LONG_PRESS: Duration = Duration::from_millis(300);
pub const DOUBLE_PRESS: Duration = Duration::from_millis(300);
pub const REPEAT_DELAY: Duration = Duration::from_millis(400);
pub const REPEAT_INTERVAL: Duration = Duration::from_millis(50);

#[must_use]
pub fn facts() -> PlatformFacts {
    PlatformFacts {
        touch_slop: Some(8.dp()),
        caret_width: Some(1.dp()),
        long_press: LONG_PRESS,
        double_press: DOUBLE_PRESS,
        key_repeat: KeyRepeat::new(REPEAT_DELAY, REPEAT_INTERVAL),
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
