// SPDX-License-Identifier: GPL-3.0-only
use lavashell::keyboard::{Placement, Side};
use lavashell::prelude::*;
use lavashell_example_support::Recording;
use lavashell_on_screen_keyboard::{
    CopyKey, Error, copy,
    model::{Appearance, Demo, docked},
    scene::{self, FIELD},
    stage::{QUERY, Stage, touch},
};

/// The recorded frames of one sequence. The frames are numbered from the first step of the sequence and saved in a
/// directory named after the sequence.
struct Sequence<'a> {
    name: &'static str,
    recording: &'a Recording,
    index: usize,
}

impl<'a> Sequence<'a> {
    fn new(name: &'static str, recording: &'a Recording) -> Self {
        Self {
            name,
            recording,
            index: 0,
        }
    }

    /// Runs `step` and records every frame until the scene comes to rest.
    fn record(
        &mut self,
        stage: &mut Stage,
        step: impl FnOnce(&mut Stage) -> Result<(), Error>,
    ) -> Result<(), Error> {
        step(stage)?;
        let directory = self.recording.directory.join(self.name);
        let index = &mut self.index;
        Ok(stage
            .offscreen
            .record_until_rest(self.recording.interval, |offscreen| {
                let saved = offscreen.save(directory.join(format!("{index:03}.png")));
                *index += 1;
                saved
            })?)
    }
}

fn still(stage: &mut Stage, recording: &Recording, name: &str) -> Result<(), Error> {
    Ok(stage
        .offscreen
        .save(recording.directory.join(format!("{name}.png")))?)
}

fn finger(id: TouchId, at: Point) -> TouchSample {
    TouchSample {
        id,
        at,
        kind: PointerKind::Finger,
    }
}

pub fn record(recording: &Recording) -> Result<(), Error> {
    let mut stage = Stage::new(Demo::new(Appearance::Night)?)?;
    let field_end = stage.field_end()?;
    Sequence::new("show", recording).record(&mut stage, |stage| stage.tap_at(field_end))?;
    stage.type_text(QUERY)?;
    stage.settle()?;

    let outside = outside_field(&stage)?;
    Sequence::new("hide", recording).record(&mut stage, |stage| stage.tap_at(outside))?;

    stage.focus_field()?;
    stage.settle()?;
    previews(&mut stage, recording)?;
    dock(&mut stage, recording)?;

    stage.tap(copy(CopyKey::Back))?;
    stage.offscreen.view.provide(&REDUCED_MOTION, true);
    stage.settle()?;
    still(&mut stage, recording, "reduced-motion-before")?;
    stage.tap_at(field_end)?;
    stage.settle()?;
    still(&mut stage, recording, "reduced-motion-after")
}

/// The point halfway between the bottom of the search field and the top of the keyboard, centered across the
/// screen.
fn outside_field(stage: &Stage) -> Result<Point, Error> {
    let field = (stage.offscreen.view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
    let panel = stage.panel()?.bounds;
    let below_field = field.y + field.height;
    let width = scene::screen().width;
    Ok(Rect::new(Dp::ZERO, below_field, width, panel.y - below_field).center())
}

fn previews(stage: &mut Stage, recording: &Recording) -> Result<(), Error> {
    let (y, u) = (stage.key("y")?, stage.key("u")?);
    let mut preview = Sequence::new("preview", recording);
    preview.record(stage, |stage| {
        stage.input(InputEvent::TouchDown(finger(touch(1), y)))
    })?;
    preview.record(stage, |stage| {
        stage.input(InputEvent::TouchUp(finger(touch(1), y)))
    })?;
    stage.settle()?;

    let mut previews = Sequence::new("previews", recording);
    previews.record(stage, |stage| {
        stage.input(InputEvent::TouchDown(finger(touch(1), y)))
    })?;
    previews.record(stage, |stage| {
        stage.input(InputEvent::TouchUp(finger(touch(1), y)))?;
        stage.input(InputEvent::TouchDown(finger(touch(2), u)))
    })?;
    previews.record(stage, |stage| {
        stage.input(InputEvent::TouchUp(finger(touch(2), u)))
    })?;
    stage.settle()
}

/// The open panel moving into the dock on the left and back to the full width.
fn dock(stage: &mut Stage, recording: &Recording) -> Result<(), Error> {
    let mut dock = Sequence::new("dock", recording);
    for placement in [docked(Side::Left), Placement::FullWidth] {
        dock.record(stage, |stage| {
            stage.demo.placement.set(placement);
            Ok(stage.offscreen.frame()?)
        })?;
    }
    Ok(())
}
