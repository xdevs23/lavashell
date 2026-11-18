// SPDX-License-Identifier: GPL-3.0-only
use crate::PortalError;
use futures_lite::StreamExt;
use std::collections::HashMap;
use zbus::message::Sequence;
use zbus::proxy::SignalStream;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

// https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
pub(crate) const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub(crate) const PATH: &str = "/org/freedesktop/portal/desktop";
pub(crate) const INTERFACE: &str = "org.freedesktop.portal.Settings";

pub struct SettingValue(OwnedValue);

impl SettingValue {
    #[must_use]
    pub fn to_u32(&self) -> Option<u32> {
        u32::try_from(&self.0).ok()
    }
}

pub struct SettingChange {
    pub key: String,
    pub value: SettingValue,
}

pub struct WatchedNamespace {
    pub values: HashMap<String, SettingValue>,
    pub changes: SettingChanges,
}

pub struct SettingChanges {
    namespace: &'static str,
    signals: SignalStream<'static>,
    read_at: Sequence,
}

impl SettingChanges {
    pub async fn next_change(&mut self) -> Option<Result<SettingChange, PortalError>> {
        while let Some(message) = self.signals.next().await {
            if message.recv_position() <= self.read_at {
                continue;
            }
            let (namespace, key, value) =
                match message.body().deserialize::<(String, String, OwnedValue)>() {
                    Ok(change) => change,
                    Err(error) => return Some(Err(PortalError::ReadSettingChange(error))),
                };
            if namespace == self.namespace {
                return Some(Ok(SettingChange {
                    key,
                    value: SettingValue(value),
                }));
            }
        }
        None
    }
}

pub async fn watch_namespace(namespace: &'static str) -> Result<WatchedNamespace, PortalError> {
    let connection = Connection::session()
        .await
        .map_err(PortalError::ConnectToSessionBus)?;
    let proxy = Proxy::new(&connection, DESTINATION, PATH, INTERFACE)
        .await
        .map_err(PortalError::AddressSettingsPortal)?;
    let signals = proxy
        .receive_signal("SettingChanged")
        .await
        .map_err(PortalError::WatchSettingChanges)?;
    let reply = proxy
        .call_method("ReadAll", &(vec![namespace],))
        .await
        .map_err(PortalError::ReadSettings)?;
    let mut namespaces = reply
        .body()
        .deserialize::<HashMap<String, HashMap<String, OwnedValue>>>()
        .map_err(PortalError::ReadSettings)?;
    let values = namespaces.remove(namespace).unwrap_or_default();
    Ok(WatchedNamespace {
        values: (values.into_iter())
            .map(|(key, value)| (key, SettingValue(value)))
            .collect(),
        changes: SettingChanges {
            namespace,
            signals,
            read_at: reply.recv_position(),
        },
    })
}

#[cfg(test)]
mod tests;
