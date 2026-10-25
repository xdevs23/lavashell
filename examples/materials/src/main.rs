// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod profile;

use clap::Parser;
use lavashell::prelude::*;
use lavashell_example_support::{Capture, ExampleArguments};
use lavashell_materials_example::{CopyKey, Error, copy, model::Demo, ui};
use profile::Profile;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
struct Arguments {
    #[command(flatten)]
    example: ExampleArguments,
    #[arg(long)]
    fullscreen: bool,
    #[arg(long)]
    autoplay: bool,
    #[arg(long, value_enum, default_value_t = Profile::Srgb)]
    profile: Profile,
    #[arg(long, value_name = "FILE")]
    screenshot: Option<PathBuf>,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let demo = Demo::new(arguments.autoplay)?;
    let request = arguments.profile.request();
    demo.request.set(request);

    if let Some(path) = &arguments.screenshot {
        let state = demo.clone();
        let mut view = View::new(lavashell_example_support::platform(), move |u| {
            ui::compose(u, &state);
        });
        let mut capture = Capture::new()?;
        let size = capture.preferred_size(&mut view)?;
        return Ok(capture.save(&mut view, Viewport::new(size, Density::ONE), path)?);
    }

    let state = demo.clone();
    let app = arguments
        .example
        .app()?
        .output_observer(move |_, info| state.output.set(Some(info)));
    Ok(lavashell_example_support::run(app, move |app| {
        app.window(
            Window::new(copy(CopyKey::Title), "lavashell.materials")
                .fullscreen(arguments.fullscreen)
                .output(request),
            move |u| ui::compose(u, &demo),
        )
    })?)
}
