// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, Error, compose, copy, model::Taskbar, script::CUES};
use lavashell::prelude::*;
use lavashell_example_support::{
    FrameInterval, Recording, Start, Steps, default_app, write_capture,
};
use lavashell_test_platform::{HostScript, RecordedFrame};

/// Records one cycle of the script in a window of `size` on the test platform, as numbered PNG files in the directory
/// set in `recording`.
pub fn record(recording: &Recording, size: Size) -> Result<(), Error> {
    for (index, (_, frame)) in record_cues(recording.interval, size)?
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

/// Records one cycle of the script in a window of `size` on the test platform, a frame each `interval`. Each cue
/// plays once the scene has come to rest after the cue before. Returns every frame from the first cue until the scene
/// comes to rest after the last cue, with the index of the cue in [`CUES`] that the frame shows.
pub(crate) fn record_cues(
    interval: FrameInterval,
    size: Size,
) -> Result<Vec<(usize, RecordedFrame)>, Error> {
    let taskbar = Taskbar::new()?;
    let mut steps = Steps::new(taskbar.clone());
    for (index, cue) in CUES.into_iter().enumerate() {
        steps.push(Start::at_rest(), move |view, _, taskbar| {
            cue.apply(taskbar, view)
        });
        steps.mark(Start::NEXT, index);
    }
    steps.record(
        app(default_app()?, &taskbar, size, false)?,
        HostScript::new(Density::ONE).record_until_rest(interval),
    )
}

/// `app` with `taskbar` in a window of `size`, which covers the screen when `fullscreen` is set.
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
