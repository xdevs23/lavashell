// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_platform::SettingsEvent;
use std::pin::Pin;

pub trait SettingsSource: Send {
    fn watch(self: Box<Self>, report: SettingsReporter) -> SettingsWatch;
}

pub type SettingsReporter = Box<dyn Fn(SettingsEvent)>;

pub type SettingsWatch = Pin<Box<dyn Future<Output = ()>>>;
