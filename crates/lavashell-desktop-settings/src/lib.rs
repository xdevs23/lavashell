// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

pub mod register;
mod service;

pub use lavashell_desktop::ServiceWatch;

use lavashell_core::preferences::Appearance;
use lavashell_store::Store;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopSetting {
    Appearance(Appearance),
}

pub trait DesktopSettingSource: Send + Sync {
    fn watch(self: Arc<Self>, report: SettingsReporter) -> ServiceWatch;
}

pub type SettingsReporter = Box<dyn Fn(DesktopSetting)>;

#[derive(Default)]
struct SettingsLayer {
    sources: Vec<Arc<dyn DesktopSettingSource>>,
}

pub fn add_source(store: &mut Store, source: Arc<dyn DesktopSettingSource>) {
    lavashell_desktop::register_in_layers(store, |layers| {
        layers.layer(|settings: &mut SettingsLayer| settings.sources.push(source));
    });
}
