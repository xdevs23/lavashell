// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod adb;
mod command_step;
mod device;
mod emulation;
mod emulator;
mod error;
mod instrumentation;
mod package_alignment;
mod run_log;
mod signed_package;
mod signing_key;

use clap::Parser;
use emulation::{RoundTripSettings, run_round_trips};
use error::NextStep;
use lavashell_core::error::message_chain;
use lavashell_process_scope::{RunOutcome, run_ending_groups_on_termination};
use package_alignment::{PackageAlignment, PageSize};
use signed_package::BuildTools;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[arg(long, help = "Unsigned package of the round trips")]
    apk: PathBuf,
    #[arg(long, help = "SDK with the build tools")]
    sdk: PathBuf,
    #[arg(long, help = "Version of the build tools")]
    build_tools: String,
    #[arg(long, help = "SDK with the emulator")]
    emulator_sdk: PathBuf,
    #[arg(long, help = "ABI of the system image")]
    abi: String,
    #[arg(long, help = "Page size in bytes of the package alignment")]
    page_size: PageSize,
    #[arg(long, help = "Alignment in bytes of the files in the package")]
    alignment: u32,
    #[arg(long, help = "Component name of the instrumentation runner")]
    runner: String,
    #[arg(
        long,
        default_value = "swiftshader_indirect",
        help = "GPU mode of the emulator"
    )]
    gpu: String,
    #[arg(long, help = "Write the results as JSON lines")]
    output: Option<PathBuf>,
    #[arg(long, help = "Directory for the data of the emulator")]
    work: Option<PathBuf>,
}

impl From<Arguments> for RoundTripSettings {
    fn from(arguments: Arguments) -> Self {
        Self {
            unsigned_package: arguments.apk,
            build_tools: BuildTools::in_sdk(&arguments.sdk, &arguments.build_tools),
            alignment: PackageAlignment {
                page_size: arguments.page_size,
                file_alignment: arguments.alignment,
            },
            emulator_sdk: arguments.emulator_sdk,
            abi: arguments.abi,
            gpu_mode: arguments.gpu,
            runner: arguments.runner,
            output: arguments.output,
            work_parent: arguments.work.unwrap_or_else(std::env::temp_dir),
        }
    }
}

fn main() -> ExitCode {
    let settings = RoundTripSettings::from(Arguments::parse());
    match run_ending_groups_on_termination(|| run_round_trips(&settings)) {
        Ok(RunOutcome::Finished(Ok(()))) => ExitCode::SUCCESS,
        Ok(RunOutcome::Terminated(termination)) => {
            eprintln!("{termination}");
            termination.exit_code()
        }
        Ok(RunOutcome::Finished(Err(error))) => {
            eprintln!("{}\n{}", message_chain(&error), error.next_step());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!(
                "{}\n{}",
                message_chain(&error),
                NextStep::after_process_error(&error)
            );
            ExitCode::FAILURE
        }
    }
}
