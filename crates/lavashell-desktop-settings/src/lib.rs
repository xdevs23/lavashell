// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use lavashell_core::preferences::Appearance;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopSetting {
    Appearance(Appearance),
}

pub trait DesktopSettingSource: Send + Sync {
    fn watch(self: Arc<Self>, report: SettingsReporter) -> SettingsWatch;
}

pub type SettingsReporter = Box<dyn Fn(DesktopSetting)>;

pub type SettingsWatch = Pin<Box<dyn Future<Output = ()>>>;
