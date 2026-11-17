// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]
mod error;
mod islands;
mod look;
mod model;
mod paint;
mod player;
mod script;
mod tag;
#[cfg(test)]
mod tests;

use clap::Parser;
use error::Error;
use lavashell::core::surface::SurfaceId;
use lavashell::prelude::*;
use lavashell_example_support::{ExampleArguments, RecordingArguments, SizeArguments, report};
use model::Taskbar;
use script::Script;
use std::{
    cell::{Cell, OnceCell, RefCell},
    process::ExitCode,
    rc::Rc,
    time::Duration,
};

include!(concat!(env!("OUT_DIR"), "/copy.rs"));

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
    #[command(flatten)]
    recording: RecordingArguments,
}

fn main() -> ExitCode {
    lavashell_example_support::exit_code(run())
}

fn run() -> Result<(), Error> {
    let arguments = Arguments::parse();
    let size = arguments.size.size().unwrap_or(look::SCREEN);
    if let Some(recording) = arguments.recording.recording() {
        return player::record(&recording, size, arguments.example.theme());
    }

    let taskbar = Taskbar::new(arguments.example.theme())?;
    let mut app = arguments.example.app()?;
    if arguments.autoplay {
        let autoplay = Autoplay::new(app.view_actions(), taskbar.clone());
        app = app.view_observer(move |surface, _, _, view| autoplay.observe(surface, view));
    }

    let app = player::app(app, &taskbar, size, arguments.fullscreen)?;
    Ok(lavashell_example_support::run(app)?)
}

struct Autoplay {
    script: RefCell<Script>,
    taskbar: Taskbar,
    actions: ViewActions,
    timer: OnceCell<Timer>,
    queued: Cell<bool>,
}

impl Autoplay {
    fn new(actions: ViewActions, taskbar: Taskbar) -> Rc<Self> {
        Rc::new(Self {
            script: RefCell::new(Script::default()),
            taskbar,
            actions,
            timer: OnceCell::new(),
            queued: Cell::new(false),
        })
    }

    fn observe(self: &Rc<Self>, surface: SurfaceId, view: &View) {
        if self.queued.get() || view.motion().is_active() {
            return;
        }
        self.queued.set(true);
        let timer = self.timer.get_or_init(|| {
            let autoplay = Rc::downgrade(self);
            view.motion().timer(move || {
                if let Some(autoplay) = autoplay.upgrade() {
                    autoplay.queue(surface);
                }
            })
        });
        timer.start(pause(view.motion()));
    }

    fn queue(self: &Rc<Self>, surface: SurfaceId) {
        let autoplay = self.clone();
        self.actions
            .dispatch(surface, move |view| autoplay.play(view));
    }

    fn play(&self, view: &mut View) {
        self.queued.set(false);
        if let Err(error) = self.script.borrow_mut().play(&self.taskbar, view) {
            report("taskbar script", error);
        }
    }
}

fn pause(motion: &Motion) -> Duration {
    (motion.skipped_rest()).map_or(Duration::ZERO, |rest| rest.saturating_sub(motion.now()))
}

fn compose(ui: &mut Ui, taskbar: &Taskbar) {
    let size = ui.viewport().size;
    ui.provide(&TEXT_STYLE, look::body(), |ui| {
        ui.canvas(size, Modifier::new(), paint::map);
        ui.column(
            Modifier::new()
                .fill_max_width()
                .fill_max_height()
                .padding(20.dp()),
            |ui| {
                islands::theme(ui, taskbar);
                ui.space(Modifier::new());
                islands::bar(ui, taskbar);
            },
        );
    });
}
