// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::platform::KeyRepeat;
use lavashell::prelude::*;
use std::time::Duration;

#[must_use]
pub fn platform() -> PlatformFacts {
    PlatformFacts {
        slop: None,
        caret_width: None,
        long_press: Duration::from_millis(300),
        double_press: Duration::from_millis(300),
        key_repeat: KeyRepeat::new(Duration::from_millis(400), Duration::from_millis(50)),
        text_size: 14.sp(),
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
