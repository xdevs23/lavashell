// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell::prelude::*;
use lavashell_example_support::{Capture, ExampleArguments, SizeArguments};
use lavashell_music_player::{Error, music};
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
    let icons = music::Icons::decode()?;

    if let Some(path) = &arguments.screenshot {
        let mut view = View::new(lavashell_example_support::platform(), move |ui| {
            music::music(ui, &model, icons);
        });
        let mut capture = Capture::new()?;
        let size = match arguments.size.size() {
            Some(size) => size,
            None => capture.preferred_size(&mut view)?,
        };
        return Ok(capture.save(&mut view, Viewport::new(size, Density::ONE), path)?);
    }

    let tick = model.clone();
    let app = arguments.example.app()?.on_tick(move |elapsed| {
        if tick.with(|m| m.playing) {
            tick.update(|m| m.advance(elapsed));
        }
    });
    Ok(lavashell_example_support::run(app, move |app| {
        if !arguments.layer || arguments.both {
            let state = model.clone();
            app.window(
                (arguments.size).window(Window::new(
                    music::copy(music::CopyKey::Title),
                    "lavashell.music",
                )),
                move |ui| music::music(ui, &state, icons),
            )?;
        }
        if arguments.layer || arguments.both {
            let state = model.clone();
            app.layer(LayerSurface::new("lavashell-music"), move |ui| {
                music::panel(ui, &state, icons);
            });
        }
        Ok(())
    })?)
}
