// SPDX-License-Identifier: GPL-3.0-only
use std::io;
use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestBusError {
    #[error("the dbus-daemon could not start")]
    StartBusDaemon(#[source] io::Error),
    #[error("the address of the private bus could not be read")]
    ReadBusAddress(#[source] io::Error),
    #[error("the dbus-daemon printed no address")]
    NoBusAddress,
    #[error("the child test could not run")]
    RunChildTest(#[source] io::Error),
    #[error("the child test ended with {0}")]
    ChildTestFailed(ExitStatus),
}
