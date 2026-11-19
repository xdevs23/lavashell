// SPDX-License-Identifier: GPL-3.0-only
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use tempfile::TempDir;

use super::TestBusError;

const BUS_CONFIG: &str = include_str!("bus.xml");

struct BusDaemon {
    process: Child,
    _runtime_directory_removed_after_the_process: TempDir,
}

impl Drop for BusDaemon {
    fn drop(&mut self) {
        if let Err(error) = self.process.kill() {
            eprintln!("the private bus did not stop: {error}");
        }
        if let Err(error) = self.process.wait() {
            eprintln!("the private bus was not reaped: {error}");
        }
    }
}

pub(super) struct PrivateBus {
    _daemon_stopped_with_the_bus: BusDaemon,
    address: String,
}

impl PrivateBus {
    pub(super) fn start(daemon_environment: &[(&str, &OsStr)]) -> Result<Self, TestBusError> {
        let runtime_directory = TempDir::new().map_err(TestBusError::CreateRuntimeDirectory)?;
        let config = runtime_directory.path().join("bus.conf");
        std::fs::write(&config, BUS_CONFIG).map_err(TestBusError::WriteBusConfig)?;
        let process = Command::new("dbus-daemon")
            .arg("--config-file")
            .arg(&config)
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .env("XDG_RUNTIME_DIR", runtime_directory.path())
            .envs(daemon_environment.iter().copied())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(TestBusError::StartBusDaemon)?;
        let mut daemon = BusDaemon {
            process,
            _runtime_directory_removed_after_the_process: runtime_directory,
        };
        let output = daemon
            .process
            .stdout
            .take()
            .ok_or(TestBusError::NoBusAddress)?;
        let address = BufReader::new(output)
            .lines()
            .next()
            .ok_or(TestBusError::NoBusAddress)?
            .map_err(TestBusError::ReadBusAddress)?;
        Ok(Self {
            _daemon_stopped_with_the_bus: daemon,
            address,
        })
    }

    pub(super) fn address(&self) -> &str {
        &self.address
    }
}

#[cfg(test)]
mod tests;
