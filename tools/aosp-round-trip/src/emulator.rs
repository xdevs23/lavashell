// SPDX-License-Identifier: GPL-3.0-only
use crate::device::{AVD_NAME, DeviceEnvironment};
use crate::error::{CommandStep, RoundTripError};
use crate::run_log::RunLog;
use lavashell_process_scope::ScopedChild;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn start_emulator(
    emulator_sdk: &Path,
    gpu_mode: &str,
    environment: &DeviceEnvironment,
    log: &RunLog,
) -> Result<ScopedChild, RoundTripError> {
    let mut command = Command::new(emulator_sdk.join("emulator/emulator"));
    let _configured = environment
        .apply(&mut command)
        .args(["-avd", AVD_NAME])
        .args(["-no-window", "-no-audio", "-no-boot-anim"])
        .args(["-no-snapshot", "-no-metrics"])
        .args(["-accel", "on"])
        .args(["-gpu", gpu_mode])
        .args(["-feature", "GuestAngle"])
        .stdin(Stdio::null())
        .stdout(log.duplicate_handle()?)
        .stderr(log.duplicate_handle()?);
    ScopedChild::spawn(&mut command).map_err(RoundTripError::command(CommandStep::StartEmulator))
}
