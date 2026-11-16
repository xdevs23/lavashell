// SPDX-License-Identifier: GPL-3.0-only
use crate::error::{CommandStep, RoundTripError};
use lavashell_process_scope::run_checked;
use std::process::Command;

pub fn run_step(step: CommandStep, command: &mut Command) -> Result<(), RoundTripError> {
    run_checked(command).map_err(RoundTripError::command(step))
}
