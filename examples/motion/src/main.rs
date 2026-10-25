// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
use clap::Parser;
use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, SizeArguments, report_failure};
use lavashell_motion::{CopyKey, Error, copy, model::Demo, ui};
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
    let demo = Demo::new(arguments.autoplay)?;

    let state = demo.clone();
    let app = arguments
        .example
        .app()?
        .on_frame(move |frame| report_failure("autoplay", state.step(frame)));
    let state = demo.clone();
    let show_render_cost = move |report: FrameReport| {
        if let Some(submission) = report.submission {
            state
                .render_ms
                .set(submission.render_cost.as_secs_f64() * 1000.0);
        }
    };

    let size = arguments.size.size().unwrap_or(ui::WINDOW);
    Ok(lavashell_example_support::run_observed(
        app,
        show_render_cost,
        move |app| {
            app.window(
                Window::new(copy(CopyKey::Title), "lavashell.motion")
                    .size(size.width, size.height)
                    .fullscreen(arguments.fullscreen),
                move |ui| ui::compose(ui, &demo),
            )
        },
    )?)
}
