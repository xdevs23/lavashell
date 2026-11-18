// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell_example_support::{ExampleArguments, SizeArguments};
use lavashell_music_player::{Error, Surfaces, build_app};
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    size: SizeArguments,
    #[arg(long, value_enum, default_value_t = Surfaces::Window)]
    surfaces: Surfaces,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let app = build_app(
        &arguments.example,
        arguments.size.size(),
        arguments.surfaces,
    )?;
    Ok(lavashell_example_support::run(app)?)
}
