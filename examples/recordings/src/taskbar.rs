// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::{FrameInterval, Recording, Steps, write_capture};
use lavashell::prelude::*;
use lavashell_example_support::{ThemeChoice, default_app};
use lavashell_taskbar::{app, model::Taskbar, script::CUES};
use lavashell_test_platform::{HostScript, RecordedFrame};

pub fn record(recording: &Recording, size: Option<Size>, theme: ThemeChoice) -> Result<(), Error> {
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

fn record_cues(
    interval: FrameInterval,
    size: Option<Size>,
    theme: ThemeChoice,
) -> Result<Vec<(usize, RecordedFrame)>, Error> {
    let taskbar = Taskbar::new(theme)?;
    let mut steps = Steps::new(taskbar.clone());
    for (index, cue) in CUES.into_iter().enumerate() {
        steps.push_at_rest_and_mark(index, move |view, _, taskbar| Ok(cue.apply(taskbar, view)?));
    }
    steps.record(
        app(default_app()?, &taskbar, size, false)?,
        HostScript::new(Density::ONE).record_until_rest(interval),
    )
}

#[cfg(test)]
mod tests;
