// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_core::preferences::Appearance;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopSetting {
    Appearance(Appearance),
}

pub trait DesktopSettingSource: Send {
    fn watch(self: Box<Self>, report: SettingsReporter) -> SettingsWatch;
}

pub type SettingsReporter = Box<dyn Fn(DesktopSetting)>;

pub type SettingsWatch = Pin<Box<dyn Future<Output = ()>>>;
