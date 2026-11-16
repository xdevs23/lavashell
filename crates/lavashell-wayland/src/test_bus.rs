// SPDX-License-Identifier: GPL-3.0-only

use crate::test_error::TestError;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use test_error::TestBusError;

pub(crate) mod test_error;

const CHILD_MARKER: &str = "LAVASHELL_PRIVATE_BUS_CHILD";

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
    address_variables: &[&str],
    child: fn() -> Result<(), TestError>,
) -> Result<(), TestError> {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return child();
    }
    let test = std::thread::current()
        .name()
        .ok_or(TestBusError::UnnamedTestThread)?
        .to_owned();
    let executable = std::env::var_os("DBUS_DAEMON").unwrap_or_else(|| "dbus-daemon".into());
    let mut daemon = BusProcess(
        Command::new(executable)
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(TestBusError::StartBusDaemon)?,
    );
    let output = daemon.0.stdout.take().ok_or(TestBusError::NoBusAddress)?;
    let mut address = String::new();
    let read = BufReader::new(output)
        .read_line(&mut address)
        .map_err(TestBusError::ReadBusAddress)?;
    if read == 0 {
        return Err(TestBusError::NoBusAddress.into());
    }
    let executable = std::env::current_exe().map_err(TestError::CurrentExecutable)?;
    let result = Command::new(executable)
        .args(["--exact", &test, "--test-threads=1"])
        .env(CHILD_MARKER, "1")
        .envs(
            address_variables
                .iter()
                .map(|variable| (variable, address.trim())),
        )
        .stderr(Stdio::inherit())
        .output()
        .map_err(TestBusError::RunChildTest)?;
    let summary = String::from_utf8_lossy(&result.stdout);
    if !result.status.success() {
        eprint!("{summary}");
        return Err(TestBusError::ChildTestFailed(result.status).into());
    }
    if !summary
        .lines()
        .any(|line| line.starts_with("test result: ok. 1 passed;"))
    {
        eprint!("{summary}");
        return Err(TestBusError::ChildTestNotRunOnce(test).into());
    }
    Ok(())
}
