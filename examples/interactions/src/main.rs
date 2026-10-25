// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod copy;
mod error;
mod model;
mod samples;
mod shape;
mod states;
mod text;
mod transfers;
mod ui;

use clap::{CommandFactory, FromArgMatches, Parser};
use copy::{CopyKey, copy};
use error::Error;
use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, SizeArguments};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    size: SizeArguments,
    #[arg(long)]
    fullscreen: bool,
    #[arg(long)]
    shaped: bool,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let matches = Arguments::command().get_matches();
    let arguments = Arguments::from_arg_matches(&matches)
        .unwrap_or_else(|error| error.format(&mut Arguments::command()).exit());
    let shaped = arguments.shaped;
    let shared_samples = shaped
        .then(|| std::env::var_os("LAVASHELL_INTERACTIONS_SAMPLES"))
        .flatten()
        .map(PathBuf::from);
    let demo = model::Demo::new(shared_samples, ExampleArguments::inherited(&matches))?;
    let hole = shape::Hole::new()?;

    let app = arguments.example.app()?;
    let clipboard = app.clipboard();
    let actions = app.view_actions();
    let observer = demo.clone();
    let quit = demo.quit.clone();
    let app = transfers::install(app, &demo)
        .view_observer(move |surface, _, view| observer.observe(surface, view))
        .exit_when(move || quit.get());

    Ok(lavashell_example_support::run(app, move |app| {
        let window = if shaped {
            shape::window(hole)
        } else {
            (arguments.size).window(
                Window::new(copy(CopyKey::Title), "lavashell.interactions")
                    .fullscreen(arguments.fullscreen),
            )
        };
        app.window(window, move |ui| {
            if shaped {
                shape::child_compose(ui, &demo, &clipboard, &actions);
            } else {
                ui::compose(ui, &demo, &clipboard, &actions);
            }
        })
    })?)
}
