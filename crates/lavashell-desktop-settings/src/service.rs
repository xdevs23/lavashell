// SPDX-License-Identifier: GPL-3.0-only
use crate::{DesktopSetting, DesktopSettingSource, SettingsLayer};
use lavashell_actor::Outlet;
use lavashell_desktop::{DesktopService, ServiceWatch};
use lavashell_platform::{PlatformEvent, SettingsEvent};
use lavashell_store::Store;
use std::sync::Arc;

pub(crate) struct DesktopSettingsService {
    sources: Vec<Arc<dyn DesktopSettingSource>>,
}

impl DesktopSettingsService {
    pub(crate) fn from_layers(layers: &Store) -> Box<dyn DesktopService> {
        Box::new(Self {
            sources: (layers.get::<SettingsLayer>().into_iter())
                .flat_map(|settings| settings.sources.iter().cloned())
                .collect(),
        })
    }
}

impl From<DesktopSetting> for SettingsEvent {
    fn from(setting: DesktopSetting) -> Self {
        match setting {
            DesktopSetting::Appearance(appearance) => Self::Appearance(appearance),
        }
    }
}

impl DesktopService for DesktopSettingsService {
    fn into_watches(self: Box<Self>, view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        (self.sources.into_iter())
            .map(|source| {
                let view = view.clone();
                source.watch(Box::new(move |setting| {
                    let _stopped_view_ends_the_run = view.push(SettingsEvent::from(setting).into());
                }))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
