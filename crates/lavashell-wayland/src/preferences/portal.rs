// SPDX-License-Identifier: GPL-3.0-only
use super::Appearance;
use futures_lite::StreamExt;
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::ContrastPreference;
use sctk::reexports::calloop::channel::Sender;
use smithay_client_toolkit as sctk;
use std::collections::HashMap;
use zbus::{Connection, Proxy, zvariant::OwnedValue};

const NAMESPACE: &str = "org.freedesktop.appearance";

// The values of the appearance keys defined by version 2 of the Settings portal. The value 0 and every unknown value
// mean no preference:
// <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html>
const PREFER_DARK: u32 = 1;
const PREFER_LIGHT: u32 = 2;
const HIGHER_CONTRAST: u32 = 1;
const REDUCED_MOTION: u32 = 1;

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

/// Sends the appearance of the desktop to `events`, then sends it again after every change, until the receiver of
/// `events` is dropped or the bus connection ends. On a desktop without the Settings portal, the call fails at once
/// with an error for an unknown service.
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
    // The subscription is made before `ReadAll`, so that a change between the two is not lost. Signals received
    // before the `ReadAll` reply are older than the values in it and are skipped by their position.
    let mut changed = proxy.receive_signal("SettingChanged").await?;
    let reply = proxy.call_method("ReadAll", &(vec![NAMESPACE],)).await?;
    let settings = reply.body().deserialize::<Settings>()?;
    let mut values = Appearance::default();
    if let Some(settings) = settings.get(NAMESPACE) {
        for (key, value) in settings {
            apply(&mut values, NAMESPACE, key, value);
        }
    }
    if events.send(values).is_err() {
        return Ok(());
    }
    while let Some(message) = changed.next().await {
        if message.recv_position() <= reply.recv_position() {
            continue;
        }
        let (namespace, key, value) = message
            .body()
            .deserialize::<(String, String, OwnedValue)>()?;
        let before = values;
        apply(&mut values, &namespace, &key, &value);
        if values != before && events.send(values).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// Applies one setting to `values`. A setting outside the appearance namespace has no effect. An unknown value
/// means no preference.
pub(super) fn apply(values: &mut Appearance, namespace: &str, key: &str, value: &OwnedValue) {
    if namespace != NAMESPACE {
        return;
    }
    let preference = u32::try_from(value).ok();
    match key {
        "reduced-motion" => values.reduced_motion = preference == Some(REDUCED_MOTION),
        "contrast" => {
            values.contrast = if preference == Some(HIGHER_CONTRAST) {
                ContrastPreference::More
            } else {
                ContrastPreference::NoPreference
            }
        }
        "color-scheme" => {
            values.color_scheme = match preference {
                Some(PREFER_DARK) => Some(ColorScheme::Dark),
                Some(PREFER_LIGHT) => Some(ColorScheme::Light),
                _ => None,
            }
        }
        _ => {}
    }
}
