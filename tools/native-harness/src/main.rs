// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod app_declarations;
mod capture;
mod check;
mod child_lines;
mod error;
mod helper;
mod launch;
mod next_steps;
mod nix_apps;
mod process_facts;
mod program_path;
mod report;
mod run_files;
mod run_label;
mod sway;

use clap::{Args, Parser, Subcommand, ValueEnum};
use error::HarnessError;
use lavashell_error_chain::message_chain;
use lavashell_process_scope::{RunOutcome, run_ending_groups_on_termination};
use next_steps::next_step;
use run_label::RunLabel;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[command(subcommand)]
    mode: Option<Mode>,
    #[command(flatten)]
    options: CheckOptions,
}

#[derive(Subcommand)]
enum Mode {
    #[command(name = capture::SUBCOMMAND, hide = true)]
    CaptureOutput,
}

#[derive(Args)]
pub struct CheckOptions {
    #[arg(long)]
    flake: Option<String>,
    #[arg(long, default_value = "native-launch")]
    label: RunLabel,
    #[arg(long = "only", help = "limit this run to the named apps")]
    only: Vec<String>,
    #[arg(
        long,
        help = "render node of the compositor (default: chosen by the compositor)"
    )]
    drm_device: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = CompositorRenderer::Gles2)]
    renderer: CompositorRenderer,
    #[arg(
        long,
        help = "directory of the logs, screenshots and report (default: a new temporary directory)"
    )]
    output_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CompositorRenderer {
    Gles2,
    Pixman,
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    match arguments.mode {
        Some(Mode::CaptureOutput) => lavashell_output_capture::run_capture_session(),
        None => {
            match run_ending_groups_on_termination(|| check::check_every_app(&arguments.options)) {
                Ok(RunOutcome::Finished(Ok(()))) => ExitCode::SUCCESS,
                Ok(RunOutcome::Terminated(termination)) => {
                    eprintln!("{termination}");
                    termination.exit_code()
                }
                Ok(RunOutcome::Finished(Err(error))) => report_failure(&error),
                Err(error) => report_failure(&HarnessError::from(error)),
            }
        }
    }
}

fn report_failure(error: &HarnessError) -> ExitCode {
    eprintln!("{}\n{}", message_chain(error), next_step(error));
    ExitCode::FAILURE
}
