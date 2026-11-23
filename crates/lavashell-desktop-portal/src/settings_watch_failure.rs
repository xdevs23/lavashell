// SPDX-License-Identifier: GPL-3.0-only
use crate::PortalError;
use std::io::ErrorKind;
use zbus::fdo;

pub(crate) enum SettingsWatchFailure {
    SessionBusMissing,
    SettingsPortalMissing,
    Portal(PortalError),
}

impl SettingsWatchFailure {
    pub(crate) fn from_session_bus_connection(failure: zbus::Error) -> Self {
        match &failure {
            zbus::Error::Connection(cause, _)
                if matches!(
                    cause.kind(),
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused
                ) =>
            {
                Self::SessionBusMissing
            }
            _ => Self::Portal(PortalError::ConnectToSessionBus(failure)),
        }
    }

    pub(crate) fn from_settings_read(failure: zbus::Error) -> Self {
        match fdo::Error::from(failure.clone()) {
            fdo::Error::ServiceUnknown(_)
            | fdo::Error::NameHasNoOwner(_)
            | fdo::Error::UnknownInterface(_)
            | fdo::Error::UnknownMethod(_) => Self::SettingsPortalMissing,
            _ => Self::Portal(PortalError::ReadSettings(failure)),
        }
    }
}

impl From<PortalError> for SettingsWatchFailure {
    fn from(failure: PortalError) -> Self {
        Self::Portal(failure)
    }
}

#[cfg(test)]
mod tests;
