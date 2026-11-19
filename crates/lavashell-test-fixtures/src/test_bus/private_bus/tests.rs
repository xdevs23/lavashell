// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use futures_lite::future::block_on;
use std::io;
use std::path::PathBuf;
use zbus::Address;
use zbus::address::transport::{Transport, UnixSocket};
use zbus::connection::Builder;
use zbus::fdo::DBusProxy;

const INSTALLED_SERVICE: &str = "[D-BUS Service]\nName=org.example.Installed\nExec=/bin/false\n";

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    TestBus(#[from] TestBusError),
    #[error("the test could not install a service file")]
    InstallService(#[source] io::Error),
    #[error("the test could not connect to the private bus")]
    Connect(#[source] zbus::Error),
    #[error("the test could not create a proxy for the bus service")]
    CreateBusProxy(#[source] zbus::Error),
    #[error("the private bus did not list the services it can start")]
    ListActivatable(#[source] zbus::fdo::Error),
    #[error("the test could not read the address of the private bus")]
    ReadAddress(#[source] zbus::Error),
    #[error("the private bus listens on {0}, which is not a socket file")]
    NotASocketFile(String),
}

fn install_service(data_directory: &TempDir) -> Result<(), TestError> {
    let services = data_directory.path().join("dbus-1").join("services");
    std::fs::create_dir_all(&services).map_err(TestError::InstallService)?;
    std::fs::write(services.join("installed.service"), INSTALLED_SERVICE)
        .map_err(TestError::InstallService)
}

fn socket_file(bus: &PrivateBus) -> Result<PathBuf, TestError> {
    let address: Address = bus.address().parse().map_err(TestError::ReadAddress)?;
    match address.transport() {
        Transport::Unix(unix) => match unix.path() {
            UnixSocket::File(path) => Ok(path.clone()),
            _ => Err(TestError::NotASocketFile(bus.address().to_owned())),
        },
        _ => Err(TestError::NotASocketFile(bus.address().to_owned())),
    }
}

#[test]
fn the_private_bus_cannot_start_a_service_installed_on_the_machine() -> Result<(), TestError> {
    let data_directory = TempDir::new().map_err(TestError::InstallService)?;
    install_service(&data_directory)?;
    let bus = PrivateBus::start(&[("XDG_DATA_DIRS", data_directory.path().as_os_str())])?;
    let connection = block_on(
        Builder::address(bus.address())
            .map_err(TestError::Connect)?
            .build(),
    )
    .map_err(TestError::Connect)?;
    let bus_service = block_on(DBusProxy::new(&connection)).map_err(TestError::CreateBusProxy)?;
    let names =
        block_on(bus_service.list_activatable_names()).map_err(TestError::ListActivatable)?;
    assert_eq!(
        names.iter().map(|name| name.as_str()).collect::<Vec<_>>(),
        ["org.freedesktop.DBus"]
    );
    Ok(())
}

#[test]
fn the_socket_of_the_private_bus_is_removed_when_the_bus_stops() -> Result<(), TestError> {
    let bus = PrivateBus::start(&[])?;
    let socket = socket_file(&bus)?;
    assert!(socket.exists());
    drop(bus);
    assert!(!socket.exists());
    Ok(())
}
