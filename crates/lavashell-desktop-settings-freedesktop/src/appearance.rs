// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::{Appearance, ContrastPreference};
use lavashell_desktop_portal::{PortalError, SettingValue, WatchedNamespace, watch_namespace};
use lavashell_desktop_settings::{
    DesktopSetting, DesktopSettingSource, ServiceWatch, SettingsReporter,
};
use lavashell_error_chain::message_chain;
use std::str::FromStr;
use std::sync::Arc;

const NAMESPACE: &str = "org.freedesktop.appearance";

const UNSET_APPEARANCE: Appearance = Appearance {
    reduced_motion: Some(false),
    contrast: Some(ContrastPreference::NoPreference),
    color_scheme: None,
};

#[derive(Debug, thiserror::Error)]
#[error("the portal setting {0:?} is not read")]
struct UnknownPortalKey(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortalKey {
    ReducedMotion,
    Contrast,
    ColorScheme,
}

impl FromStr for PortalKey {
    type Err = UnknownPortalKey;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        match key {
            "reduced-motion" => Ok(Self::ReducedMotion),
            "contrast" => Ok(Self::Contrast),
            "color-scheme" => Ok(Self::ColorScheme),
            _ => Err(UnknownPortalKey(key.to_owned())),
        }
    }
}

impl PortalKey {
    // https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
    fn read(self, value: Option<u32>, appearance: Appearance) -> Appearance {
        match self {
            Self::ReducedMotion => Appearance {
                reduced_motion: Some(value == Some(1)),
                ..appearance
            },
            Self::Contrast => Appearance {
                contrast: Some(if value == Some(1) {
                    ContrastPreference::More
                } else {
                    ContrastPreference::NoPreference
                }),
                ..appearance
            },
            Self::ColorScheme => Appearance {
                color_scheme: match value {
                    Some(1) => Some(ColorScheme::Dark),
                    Some(2) => Some(ColorScheme::Light),
                    _ => None,
                },
                ..appearance
            },
        }
    }
}

fn read_setting(appearance: Appearance, key: &str, value: &SettingValue) -> Appearance {
    key.parse::<PortalKey>()
        .map_or(appearance, |key| key.read(value.to_u32(), appearance))
}

pub struct AppearanceSource;

impl DesktopSettingSource for AppearanceSource {
    fn watch(self: Arc<Self>, report: SettingsReporter) -> ServiceWatch {
        Box::pin(async move {
            if let Err(error) = watch_appearance(&report).await {
                log::info!(
                    "the appearance preferences of the desktop are unavailable: {}",
                    message_chain(error)
                );
            }
        })
    }
}

async fn watch_appearance(report: &SettingsReporter) -> Result<(), PortalError> {
    let WatchedNamespace {
        values,
        mut changes,
    } = watch_namespace(NAMESPACE).await?;
    let mut reported = (values.iter()).fold(UNSET_APPEARANCE, |appearance, (key, value)| {
        read_setting(appearance, key, value)
    });
    report(DesktopSetting::Appearance(reported));
    while let Some(change) = changes.next_change().await {
        let change = change?;
        let latest = read_setting(reported, &change.key, &change.value);
        if latest != reported {
            reported = latest;
            report(DesktopSetting::Appearance(reported));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
