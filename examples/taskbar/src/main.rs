// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell_example_support::{ExampleArguments, SizeArguments};
use lavashell_taskbar::{Autoplay, Error, model::Taskbar};
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
    autoplay: bool,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let taskbar = Taskbar::new(arguments.example.theme())?;
    let mut app = arguments.example.app()?;
    if arguments.autoplay {
        let autoplay = Autoplay::new(app.view_actions(), taskbar.clone());
        app = app.view_observer(move |surface, _, _, view| autoplay.observe(surface, view));
    }

    let app = lavashell_taskbar::app(app, &taskbar, arguments.size.size(), arguments.fullscreen)?;
    Ok(lavashell_example_support::run(app)?)
}
