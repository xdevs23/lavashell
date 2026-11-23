// SPDX-License-Identifier: GPL-3.0-only
use std::process::Command;

#[must_use]
pub fn bash(script: &str) -> Command {
    let mut command = Command::new("bash");
    let _command: &mut Command = command.args(["-euo", "pipefail", "-c", script]);
    command
}
