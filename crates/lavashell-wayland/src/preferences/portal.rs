// SPDX-License-Identifier: GPL-3.0-only
use crate::sctk;
use futures_lite::StreamExt;
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::{Appearance, ContrastPreference};
use sctk::reexports::calloop::channel::Sender;
use std::collections::HashMap;
use std::str::FromStr;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

const NAMESPACE: &str = "org.freedesktop.appearance";

// https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
const PREFER_DARK: u32 = 1;
const PREFER_LIGHT: u32 = 2;
const HIGHER_CONTRAST: u32 = 1;
const REDUCED_MOTION: u32 = 1;

#[derive(Debug, thiserror::Error)]
#[error("the portal setting {0:?} is not read")]
pub(super) struct UnknownPortalKey(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PortalKey {
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

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AppearanceSettings {
    reduced_motion: Option<u32>,
    contrast: Option<u32>,
    color_scheme: Option<u32>,
}

impl AppearanceSettings {
    fn read(&mut self, key: &str, value: &OwnedValue) {
        if let Ok(key) = key.parse::<PortalKey>() {
            self.set(key, u32::try_from(value).ok());
        }
    }

    pub(super) fn set(&mut self, key: PortalKey, preference: Option<u32>) {
        let setting = match key {
            PortalKey::ReducedMotion => &mut self.reduced_motion,
            PortalKey::Contrast => &mut self.contrast,
            PortalKey::ColorScheme => &mut self.color_scheme,
        };
        *setting = preference;
    }
}

pub(super) fn appearance(settings: AppearanceSettings) -> Appearance {
    Appearance {
        reduced_motion: Some(settings.reduced_motion == Some(REDUCED_MOTION)),
        contrast: Some(if settings.contrast == Some(HIGHER_CONTRAST) {
            ContrastPreference::More
        } else {
            ContrastPreference::NoPreference
        }),
        color_scheme: match settings.color_scheme {
            Some(PREFER_DARK) => Some(ColorScheme::Dark),
            Some(PREFER_LIGHT) => Some(ColorScheme::Light),
            _ => None,
        },
    }
}

pub(crate) async fn watch(events: Sender<Appearance>) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    watch_connection(&connection, events).await
}

pub(super) async fn watch_connection(
    connection: &Connection,
    events: Sender<Appearance>,
) -> zbus::Result<()> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await?;
    let mut changes = proxy.receive_signal("SettingChanged").await?;
    let reply = proxy.call_method("ReadAll", &(vec![NAMESPACE],)).await?;
    let mut namespaces = reply
        .body()
        .deserialize::<HashMap<String, HashMap<String, OwnedValue>>>()?;
    let mut settings = AppearanceSettings::default();
    for (key, value) in namespaces.remove(NAMESPACE).unwrap_or_default() {
        settings.read(&key, &value);
    }
    let mut values = appearance(settings);
    if events.send(values).is_err() {
        return Ok(());
    }
    while let Some(message) = changes.next().await {
        if message.recv_position() <= reply.recv_position() {
            continue;
        }
        let (namespace, key, value) = message
            .body()
            .deserialize::<(String, String, OwnedValue)>()?;
        if namespace != NAMESPACE {
            continue;
        }
        settings.read(&key, &value);
        let latest = appearance(settings);
        if latest != values {
            values = latest;
            if events.send(values).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}
