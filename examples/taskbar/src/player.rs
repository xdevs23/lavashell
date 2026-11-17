// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, Error, compose, copy, model::Taskbar, script::CUES};
use lavashell::prelude::*;
use lavashell_example_support::{
    FrameInterval, Recording, Steps, ThemeChoice, default_app, write_capture,
};
use lavashell_test_platform::{HostScript, RecordedFrame};

pub fn record(recording: &Recording, size: Size, theme: ThemeChoice) -> Result<(), Error> {
    for (index, (_, frame)) in record_cues(recording.interval, size, theme)?
        .into_iter()
        .enumerate()
    {
        write_capture(
            recording.directory.join(format!("{index:04}.png")),
            &frame.png,
        )?;
    }
    Ok(())
}

pub(crate) fn record_cues(
    interval: FrameInterval,
    size: Size,
    theme: ThemeChoice,
) -> Result<Vec<(usize, RecordedFrame)>, Error> {
    let taskbar = Taskbar::new(theme)?;
    let mut steps = Steps::new(taskbar.clone());
    for (index, cue) in CUES.into_iter().enumerate() {
        steps.push_at_rest_and_mark(index, move |view, _, taskbar| cue.apply(taskbar, view));
    }
    steps.record(
        app(default_app()?, &taskbar, size, false)?,
        HostScript::new(Density::ONE).record_until_rest(interval),
    )
}

pub(crate) fn app(
    mut app: App,
    taskbar: &Taskbar,
    size: Size,
    fullscreen: bool,
) -> Result<App, Error> {
    let shown = taskbar.clone();
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.taskbar")
            .size(size.width, size.height)
            .fullscreen(fullscreen),
        move |ui| compose(ui, &shown),
    )?;
    Ok(app)
}
