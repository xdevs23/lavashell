// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, Error, app, copy,
    model::{Appearance, Demo, docked},
    scene::{self, FIELD},
    stage::{self, QUERY, finger, tap, tap_field_end},
};
use lavashell::keyboard::{Placement, Side};
use lavashell::prelude::*;
use lavashell_example_support::{Recording, Start, Steps, default_app, write_capture};
use lavashell_test_platform::{HostScript, RecordedFrame};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

/// How the frames of a step are recorded.
#[derive(Clone, Copy)]
enum Take {
    /// Numbered frames in a directory named after the sequence. The frames are numbered from the first step of the
    /// sequence on.
    Sequence(&'static str),
    /// The frame at rest after the step, in a file named after the still.
    Still(&'static str),
    Unrecorded,
}

/// Records the keyboard showing, typing, hiding, previewing keys and docking on the test platform, as PNG files in
/// the directory set in `recording`.
///
/// # Errors
/// Returns an error when the scene lacks a part that a step uses, when the app fails, when the recording comes to rest
/// before every step played, or when a frame cannot be written.
pub fn record(recording: &Recording) -> Result<(), Error> {
    let demo = Demo::new(Appearance::Night)?;
    let steps = steps(&demo);
    let script = HostScript::new(scene::density()).record_until_rest(recording.interval);
    let app = app(default_app()?, &demo, scene::screen(), false)?;
    write(recording, steps.record(app, script)?)
}

fn steps(demo: &Demo) -> Steps<Demo, Take, Error> {
    let pixel = Viewport::new(scene::screen(), scene::density()).pixel();
    let mut steps = Steps::new(demo.clone());
    push_take(&mut steps, Take::Sequence("show"), move |view, _, _| {
        tap_field_end(view, pixel)
    });
    push_take(&mut steps, Take::Unrecorded, |view, _, typed| {
        (QUERY.chars()).try_for_each(|character| tap(view, &stage::key_name(typed, character)))
    });
    push_take(&mut steps, Take::Sequence("hide"), |view, _, shown| {
        let outside = outside_field(view, shown)?;
        stage::tap_at(view, outside);
        Ok(())
    });
    push_take(&mut steps, Take::Unrecorded, move |view, _, _| {
        tap_field_end(view, pixel)
    });
    previews(&mut steps);
    for placement in [docked(Side::Left), Placement::FullWidth] {
        push_take(&mut steps, Take::Sequence("dock"), move |_, _, placed| {
            placed.placement.set(placement);
            Ok(())
        });
    }
    push_take(&mut steps, Take::Unrecorded, |view, _, _| {
        tap(view, copy(CopyKey::Back))
    });
    push_take(
        &mut steps,
        Take::Still("reduced-motion-before"),
        |view, _, _| {
            view.provide(&REDUCED_MOTION, true);
            Ok(())
        },
    );
    push_take(
        &mut steps,
        Take::Still("reduced-motion-after"),
        move |view, _, _| tap_field_end(view, pixel),
    );
    steps
}

/// Adds `action` as a step once the scene is at rest, and labels the frames from the one that shows it on with `take`.
fn push_take(
    steps: &mut Steps<Demo, Take, Error>,
    take: Take,
    action: impl FnOnce(&mut View, Viewport, &Demo) -> Result<(), Error> + 'static,
) {
    steps.push(Start::at_rest(), action);
    steps.mark(Start::NEXT, take);
}

/// A key preview for one finger, then previews for a finger that lifts while the next one touches down.
fn previews(steps: &mut Steps<Demo, Take, Error>) {
    let fingers = Fingers::default();
    let (preview, previews) = (Take::Sequence("preview"), Take::Sequence("previews"));
    let gestures = [
        (preview, vec![Finger::Down(1, "y")]),
        (preview, vec![Finger::Up(1)]),
        (previews, vec![Finger::Down(1, "y")]),
        (previews, vec![Finger::Up(1), Finger::Down(2, "u")]),
        (previews, vec![Finger::Up(2)]),
    ];
    for (take, moves) in gestures {
        let fingers = fingers.clone();
        push_take(steps, take, move |view, _, _| {
            moves
                .into_iter()
                .try_for_each(|change| fingers.apply(view, change))
        });
    }
}

/// A finger that touches down on the key with a name, or lifts.
#[derive(Clone, Copy)]
enum Finger {
    Down(i32, &'static str),
    Up(i32),
}

/// The touch points that are down, at the point where each went down.
#[derive(Clone, Default)]
struct Fingers(Rc<RefCell<BTreeMap<i32, Point>>>);

impl Fingers {
    fn apply(&self, view: &mut View, change: Finger) -> Result<(), Error> {
        match change {
            Finger::Down(point, key) => {
                let at = stage::key(view, key)?;
                let _not_down_before = self.0.borrow_mut().insert(point, at);
                view.input(InputEvent::TouchDown(finger(point, at)));
            }
            Finger::Up(point) => {
                if let Some(at) = self.0.borrow_mut().remove(&point) {
                    view.input(InputEvent::TouchUp(finger(point, at)));
                }
            }
        }
        Ok(())
    }
}

/// The point halfway between the bottom of the search field and the top of the keyboard, centered across the
/// screen.
fn outside_field(view: &View, demo: &Demo) -> Result<Point, Error> {
    let field = (view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
    let panel = stage::panel(view, demo)?.bounds;
    let below_field = field.y + field.height;
    let width = scene::screen().width;
    Ok(Rect::new(Dp::ZERO, below_field, width, panel.y - below_field).center())
}

/// Writes each frame of a sequence as a numbered file into the directory of the sequence, and the last frame of each
/// still into the file of the still.
fn write(recording: &Recording, frames: Vec<(Take, RecordedFrame)>) -> Result<(), Error> {
    let mut numbered = BTreeMap::<&str, usize>::new();
    let mut stills = BTreeMap::new();
    for (take, frame) in frames {
        match take {
            Take::Sequence(name) => {
                let index = numbered.entry(name).or_default();
                let path = recording
                    .directory
                    .join(name)
                    .join(format!("{index:03}.png"));
                write_capture(path, &frame.png)?;
                *index += 1;
            }
            Take::Still(name) => {
                let _earlier_frame = stills.insert(name, frame.png);
            }
            Take::Unrecorded => {}
        }
    }
    for (name, png) in stills {
        write_capture(recording.directory.join(format!("{name}.png")), &png)?;
    }
    Ok(())
}
