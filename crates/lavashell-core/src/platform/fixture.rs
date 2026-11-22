// SPDX-License-Identifier: GPL-3.0-only
use super::PlatformFacts;
use crate::units::DpExt;
use icu_locale_core::langid;
use std::time::Duration;

pub const LONG_PRESS: Duration = super::DEFAULT_LONG_PRESS;
pub const DOUBLE_PRESS: Duration = super::DEFAULT_DOUBLE_PRESS;
pub const REPEAT_DELAY: Duration = super::DEFAULT_KEY_REPEAT_DELAY;
pub const REPEAT_INTERVAL: Duration = super::DEFAULT_KEY_REPEAT_INTERVAL;

#[must_use]
pub fn facts() -> PlatformFacts {
    PlatformFacts {
        slop: Some(8.dp()),
        caret_width: Some(1.dp()),
        locales: vec![langid!("en")],
        ..PlatformFacts::default()
    }
}
