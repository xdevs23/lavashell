// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell_example_support::ExampleArguments;
use lavashell_materials_example::{Error, OutputArguments, build_app, model::Run};
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    output: OutputArguments,
    #[arg(long)]
    autoplay: bool,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let run = Run::Live {
        autoplay: arguments.autoplay,
    };
    let app = build_app(&arguments.example, arguments.output, run)?;
    Ok(lavashell_example_support::run(app)?)
}
