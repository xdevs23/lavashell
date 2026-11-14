// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use clap::Parser;
use lavashell_example_support::{ExampleArguments, RecordingArguments, SizeArguments};
use lavashell_on_screen_keyboard::{
    Error,
    model::{Appearance, Demo},
    scene,
};
use std::process::ExitCode;

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    size: SizeArguments,
    #[arg(long)]
    fullscreen: bool,
    #[arg(long)]
    day: bool,
    #[command(flatten)]
    recording: RecordingArguments,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    if let Some(recording) = arguments.recording.recording() {
        return lavashell_on_screen_keyboard::recording::record(&recording);
    }

    let appearance = if arguments.day {
        Appearance::Day
    } else {
        Appearance::Night
    };
    let demo = Demo::new(appearance)?;
    let size = arguments.size.size().unwrap_or_else(scene::screen);
    let app = arguments.example.app()?;
    let app = lavashell_on_screen_keyboard::app(app, &demo, size, arguments.fullscreen)?;
    Ok(lavashell_example_support::run(app)?)
}
