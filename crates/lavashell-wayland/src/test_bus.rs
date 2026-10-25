// SPDX-License-Identifier: GPL-3.0-only
//! Runs a test again as a subprocess on a private D-Bus session bus, started for that test and stopped after it.

use crate::test_error::TestError;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

struct BusProcess(Child);

impl Drop for BusProcess {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            eprintln!("the private bus did not stop: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("the private bus was not reaped: {error}");
        }
    }
}

pub(crate) fn run(
    marker: &str,
    test: &str,
    child: fn() -> Result<(), TestError>,
) -> Result<(), TestError> {
    if std::env::var_os(marker).is_some() {
        return child();
    }
    let executable = std::env::var_os("DBUS_DAEMON").unwrap_or_else(|| "dbus-daemon".into());
    let mut daemon = BusProcess(
        Command::new(executable)
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(TestError::StartBusDaemon)?,
    );
    let output = daemon.0.stdout.take().ok_or(TestError::NoBusAddress)?;
    let mut address = String::new();
    let read = BufReader::new(output)
        .read_line(&mut address)
        .map_err(TestError::ReadBusAddress)?;
    if read == 0 {
        return Err(TestError::NoBusAddress);
    }
    let executable = std::env::current_exe().map_err(TestError::CurrentExecutable)?;
    let result = Command::new(executable)
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(marker, "1")
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
        .env("AT_SPI_BUS_ADDRESS", address.trim())
        .status()
        .map_err(TestError::RunChildTest)?;
    if result.success() {
        Ok(())
    } else {
        Err(TestError::ChildTestFailed(result))
    }
}
