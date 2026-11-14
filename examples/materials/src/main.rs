// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod profile;

use clap::Parser;
use lavashell_example_support::ExampleArguments;
use lavashell_materials_example::{APP_ID, Error, model::Demo};
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
    let demo = Demo::new(arguments.autoplay && arguments.screenshot.is_none())?;
    demo.request.set(arguments.profile.request());

    let app = arguments.example.app()?;
    let app = lavashell_materials_example::app(app, &demo, None, arguments.fullscreen)?;
    if let Some(path) = &arguments.screenshot {
        return Ok(lavashell_example_support::write_screenshot(
            app, APP_ID, path,
        )?);
    }
    Ok(lavashell_example_support::run(app)?)
}
