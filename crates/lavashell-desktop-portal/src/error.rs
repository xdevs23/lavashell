// SPDX-License-Identifier: GPL-3.0-only

#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error("the connection to the session bus failed")]
    ConnectToSessionBus(#[source] zbus::Error),
    #[error("the settings portal could not be addressed")]
    AddressSettingsPortal(#[source] zbus::Error),
    #[error("changes to the portal settings could not be watched")]
    WatchSettingChanges(#[source] zbus::Error),
    #[error("the portal settings could not be read")]
    ReadSettings(#[source] zbus::Error),
    #[error("a changed portal setting could not be read")]
    ReadSettingChange(#[source] zbus::Error),
}
