// SPDX-License-Identifier: GPL-3.0-only
use crate::{CopyKey, Error, compose, copy, model::Taskbar, script::CUES};
use lavashell::prelude::*;
use lavashell_example_support::{FrameInterval, Recording, Steps, write_capture};
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
    let mut steps = Steps::default();
    for (index, cue) in CUES.into_iter().enumerate() {
        let taskbar = taskbar.clone();
        steps.push(index, move |view| cue.apply(&taskbar, view));
    }
    let mut app =
        App::new(lavashell_example_support::platform()).fonts(lavashell_example_support::fonts()?);
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.taskbar").size(size.width, size.height),
        move |ui| compose(ui, &taskbar),
    )?;
    steps.record(
        app,
        HostScript::new(Density::ONE).record_until_rest(interval),
    )
}
