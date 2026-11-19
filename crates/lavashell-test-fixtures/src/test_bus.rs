// SPDX-License-Identifier: GPL-3.0-only
use std::path::PathBuf;
use std::process::{Command, Stdio};

mod error;
mod private_bus;

pub use error::TestBusError;
use private_bus::PrivateBus;

const CHILD_MARKER: &str = "LAVASHELL_PRIVATE_BUS_CHILD";

pub fn current_test_executable() -> Result<PathBuf, TestBusError> {
    std::env::current_exe().map_err(TestBusError::FindTestExecutable)
}

pub fn run<E: From<TestBusError>>(
    address_variables: &[&str],
    child: fn() -> Result<(), E>,
) -> Result<(), E> {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return child();
    }
    let test = std::thread::current()
        .name()
        .ok_or(TestBusError::UnnamedTestThread)?
        .to_owned();
    let bus = PrivateBus::start(&[])?;
    let executable = current_test_executable()?;
    let result = Command::new(executable)
        .args(["--exact", &test, "--test-threads=1"])
        .env(CHILD_MARKER, "1")
        .envs(
            address_variables
                .iter()
                .map(|variable| (variable, bus.address())),
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
