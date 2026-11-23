// SPDX-License-Identifier: GPL-3.0-only
use crate::settings::{DESTINATION, INTERFACE, PATH};
use std::collections::HashMap;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, connection::Builder};

pub struct TestSettingChange {
    pub namespace: &'static str,
    pub key: &'static str,
    pub value: u32,
}

pub struct TestSettingsPortal {
    pub read_all_reply: zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>>,
    pub changed_before_reply: Vec<TestSettingChange>,
}

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl TestSettingsPortal {
    async fn read_all(
        &mut self,
        namespaces: Vec<String>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>> {
        for change in std::mem::take(&mut self.changed_before_reply) {
            emit_setting_changed(connection, &change).await?;
        }
        Ok(
            std::mem::replace(&mut self.read_all_reply, Ok(HashMap::new()))?
                .into_iter()
                .filter(|(namespace, _)| namespaces.contains(namespace))
                .collect(),
        )
    }
}

impl TestSettingsPortal {
    pub async fn serve(self) -> zbus::Result<Connection> {
        Builder::session()?
            .name(DESTINATION)?
            .serve_at(PATH, self)?
            .build()
            .await
    }
}

pub async fn emit_setting_changed(
    connection: &Connection,
    change: &TestSettingChange,
) -> zbus::Result<()> {
    connection
        .emit_signal(
            None::<&str>,
            PATH,
            INTERFACE,
            "SettingChanged",
            &(change.namespace, change.key, OwnedValue::from(change.value)),
        )
        .await
}
