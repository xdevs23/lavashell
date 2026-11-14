// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, SizeArguments};
use lavashell_music_player::{APP_ID, Error, music};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[command(flatten)]
    size: SizeArguments,
    #[arg(long)]
    layer: bool,
    #[arg(long)]
    both: bool,
    #[arg(long, value_name = "FILE")]
    screenshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let model = State::new(music::Model::default());
    let size = arguments.size.size();

    let mut app = arguments.example.app()?;
    if let Some(path) = &arguments.screenshot {
        let app = lavashell_music_player::app(app, &model, size)?;
        return Ok(lavashell_example_support::write_screenshot(
            app, APP_ID, path,
        )?);
    }
    if !arguments.layer || arguments.both {
        app = lavashell_music_player::app(app, &model, size)?;
    }
    let tick = model.clone();
    app = app.on_tick(move |elapsed| {
        if tick.with(|m| m.playing) {
            tick.update(|m| m.advance(elapsed));
        }
    });
    if arguments.layer || arguments.both {
        let (state, icons) = (model.clone(), music::Icons::decode()?);
        app.layer(LayerSurface::new("lavashell-music"), move |ui| {
            music::panel(ui, &state, icons);
        });
    }
    Ok(lavashell_example_support::run(app)?)
}
