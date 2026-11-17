// SPDX-License-Identifier: GPL-3.0-only
use crate::adb::AdbServer;
use crate::command_step::run_step;
use crate::device::{SystemImage, prepare_device};
use crate::emulator::start_emulator;
use crate::error::{CommandStep, FileOperation, RoundTripError};
use crate::instrumentation::{RoundTripResult, json_lines, read_results};
use crate::package_alignment::PackageAlignment;
use crate::run_log::RunLog;
use crate::signed_package::{BuildTools, sign_package};
use lavashell_error_chain::message_chain;
use lavashell_process_scope::{CommandLine, ScopedChild, first_ready, run_checked_for_stdout};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const USER_UNLOCKED_EVENT: &str = "uc_finish_user_unlocking";

#[derive(Clone, Copy)]
enum BootEvent {
    EmulatorExited,
    AdbServerExited,
    WaitEnded,
}

pub struct RoundTripSettings {
    pub unsigned_package: PathBuf,
    pub build_tools: BuildTools,
    pub alignment: PackageAlignment,
    pub emulator_sdk: PathBuf,
    pub abi: String,
    pub gpu_mode: String,
    pub runner: String,
    pub output: Option<PathBuf>,
    pub work_parent: PathBuf,
}

pub fn run_round_trips(settings: &RoundTripSettings) -> Result<(), RoundTripError> {
    let work = tempfile::Builder::new()
        .prefix("lavashell-emulator-")
        .tempdir_in(&settings.work_parent)
        .map_err(RoundTripError::file(
            FileOperation::CreateTemporary,
            &settings.work_parent,
        ))?;
    let log = RunLog::create()?;
    emulate(settings, work.path(), &log)
        .and_then(|results| publish_results(&results, settings.output.as_deref()))
        .inspect_err(|_failure| log.keep_and_print_path())
}

fn publish_results(
    results: &[RoundTripResult],
    output: Option<&Path>,
) -> Result<(), RoundTripError> {
    let document = json_lines(results)?;
    print!("{document}");
    if let Some(output) = output {
        fs::write(output, &document).map_err(RoundTripError::file(FileOperation::Write, output))?;
    }
    let steps: Vec<String> = results
        .iter()
        .filter(|result| result.failed())
        .map(|result| result.step.clone())
        .collect();
    if steps.is_empty() {
        Ok(())
    } else {
        Err(RoundTripError::RoundTripsFailed { steps })
    }
}

fn emulate(
    settings: &RoundTripSettings,
    work: &Path,
    log: &RunLog,
) -> Result<Vec<RoundTripResult>, RoundTripError> {
    let package = work.join("probe.apk");
    sign_package(
        &settings.build_tools,
        settings.alignment,
        &settings.unsigned_package,
        &package,
    )?;
    let image = SystemImage {
        emulator_sdk: &settings.emulator_sdk,
        abi: &settings.abi,
    };
    let environment = prepare_device(work, &image, &settings.gpu_mode)?;
    let adb = AdbServer::start(
        &settings.emulator_sdk.join("platform-tools/adb"),
        environment,
        log,
    )?;
    let emulator = start_emulator(
        &settings.emulator_sdk,
        &settings.gpu_mode,
        adb.environment(),
        log,
    )?;
    wait_for_boot(&adb, &emulator)?;
    run_step(
        CommandStep::InstallPackage,
        adb.client().arg("install").arg("-r").arg(&package),
    )?;
    let results = run_instrumentation(&adb, &settings.runner);
    let saved = settings.output.as_ref().map_or(Ok(()), |output| {
        save_device_log(&adb, &output.with_extension("logcat"))
    });
    match (results, saved) {
        (results, Ok(())) => results,
        (Ok(_results_without_device_log), Err(save_failure)) => Err(save_failure),
        (Err(error), Err(save_failure)) => {
            eprintln!("{}", message_chain(save_failure));
            Err(error)
        }
    }
}

fn wait_for_boot(adb: &AdbServer, emulator: &ScopedChild) -> Result<(), RoundTripError> {
    let online: &[&str] = &["wait-for-device"];
    let user_unlocked: &[&str] = &[
        "shell",
        "logcat",
        "-b",
        "events",
        "-m",
        "1",
        "-s",
        USER_UNLOCKED_EVENT,
    ];
    for arguments in [online, user_unlocked] {
        let mut command = adb.client();
        let _configured = command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        let waiting = ScopedChild::spawn(&mut command)
            .map_err(RoundTripError::command(CommandStep::WaitForBoot))?;
        let event = first_ready(&[
            (emulator.exit_event(), BootEvent::EmulatorExited),
            (adb.exit_event(), BootEvent::AdbServerExited),
            (waiting.exit_event(), BootEvent::WaitEnded),
        ])
        .map_err(RoundTripError::command(CommandStep::WaitForBoot))?;
        match event {
            BootEvent::EmulatorExited => return Err(RoundTripError::EmulatorExited),
            BootEvent::AdbServerExited => return Err(RoundTripError::AdbServerExited),
            BootEvent::WaitEnded => {
                let exit = waiting
                    .wait()
                    .map_err(RoundTripError::command(CommandStep::WaitForBoot))?;
                if !exit.success() {
                    return Err(RoundTripError::BootWaitFailed {
                        command: CommandLine::from(&command),
                        exit,
                    });
                }
            }
        }
    }
    Ok(())
}

fn run_instrumentation(
    adb: &AdbServer,
    runner: &str,
) -> Result<Vec<RoundTripResult>, RoundTripError> {
    let printed =
        run_checked_for_stdout(
            adb.client()
                .args(["shell", "am", "instrument", "-w", runner]),
        )
        .map_err(RoundTripError::command(CommandStep::RunInstrumentation))?;
    let output = String::from_utf8(printed).map_err(RoundTripError::InstrumentationEncoding)?;
    read_results(&output)
}

fn save_device_log(adb: &AdbServer, path: &Path) -> Result<(), RoundTripError> {
    let device_log = run_checked_for_stdout(adb.client().args(["logcat", "-d"]))
        .map_err(RoundTripError::command(CommandStep::ReadDeviceLog))?;
    fs::write(path, device_log).map_err(RoundTripError::file(FileOperation::Write, path))
}
