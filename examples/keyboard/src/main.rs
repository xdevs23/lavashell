// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod frames;

use clap::Parser;
use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, RecordingArguments, SizeArguments};
use lavashell_on_screen_keyboard::{
    CopyKey, Error, copy,
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
        return frames::record(&recording);
    }

    let appearance = if arguments.day {
        Appearance::Day
    } else {
        Appearance::Night
    };
    let demo = Demo::new(appearance)?;
    let arrow_keys = demo.clone();
    let app = (arguments.example.app()?).input_filter(move |_, input| arrow_keys.route(input));

    let size = arguments.size.size().unwrap_or_else(scene::screen);
    Ok(lavashell_example_support::run(app, move |app| {
        app.window(
            Window::new(copy(CopyKey::Title), "lavashell.keyboard")
                .size(size.width, size.height)
                .fullscreen(arguments.fullscreen),
            move |ui| scene::compose(ui, &demo),
        )
    })?)
}
