// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use futures_lite::future::block_on;
use zbus::connection::Builder;

fn connect(address: &str) -> zbus::Result<zbus::Connection> {
    block_on(Builder::address(address)?.build())
}

fn read_settings(reply: fdo::Error) -> SettingsWatchFailure {
    SettingsWatchFailure::from_settings_read(zbus::Error::FDO(Box::new(reply)))
}

#[test]
fn a_bus_address_without_a_socket_means_the_session_bus_is_missing() {
    let socket = std::env::temp_dir()
        .join(format!("lavashell-no-session-bus-{}", std::process::id()))
        .join("bus");
    let address = format!("unix:path={}", socket.display());
    assert!(matches!(
        connect(&address).map_err(SettingsWatchFailure::from_session_bus_connection),
        Err(SettingsWatchFailure::SessionBusMissing)
    ));
}

#[test]
fn a_bus_address_without_a_listener_means_the_session_bus_is_missing() {
    let address = format!(
        "unix:abstract=lavashell-no-session-bus-{}",
        std::process::id()
    );
    assert!(matches!(
        connect(&address).map_err(SettingsWatchFailure::from_session_bus_connection),
        Err(SettingsWatchFailure::SessionBusMissing)
    ));
}

#[test]
fn a_rejected_authentication_fails_the_session_bus_connection() {
    let failure = zbus::Error::Handshake("the bus rejected the authentication".to_owned());
    assert!(matches!(
        SettingsWatchFailure::from_session_bus_connection(failure),
        SettingsWatchFailure::Portal(PortalError::ConnectToSessionBus(_))
    ));
}

#[test]
fn a_reply_naming_an_unknown_service_interface_or_method_means_the_settings_portal_is_missing() {
    for reply in [
        fdo::Error::ServiceUnknown("the bus does not know the service".to_owned()),
        fdo::Error::NameHasNoOwner("the bus name has lost its owner".to_owned()),
        fdo::Error::UnknownInterface("the portal does not serve the settings".to_owned()),
        fdo::Error::UnknownMethod("the portal does not read settings".to_owned()),
    ] {
        assert!(matches!(
            read_settings(reply),
            SettingsWatchFailure::SettingsPortalMissing
        ));
    }
}

#[test]
fn a_failed_reply_fails_the_settings_read() {
    assert!(matches!(
        read_settings(fdo::Error::Failed("the backend stopped".to_owned())),
        SettingsWatchFailure::Portal(PortalError::ReadSettings(_))
    ));
}
