// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    CopyKey, Error, copy,
    model::{Appearance, Demo, docked},
    scene::{self, FIELD},
    stage::{self, QUERY, touch},
};
use lavashell::keyboard::{Placement, Side};
use lavashell::prelude::*;
use lavashell_example_support::{Recording, Steps, write_capture};
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
    let screen = scene::screen();
    let mut app =
        App::new(lavashell_example_support::platform()).fonts(lavashell_example_support::fonts()?);
    app.window(
        Window::new(copy(CopyKey::Title), "lavashell.keyboard").size(screen.width, screen.height),
        move |ui| scene::compose(ui, &demo),
    )?;
    let script = HostScript::new(scene::density()).record_until_rest(recording.interval);
    write(recording, steps.record(app, script)?)
}

fn steps(demo: &Demo) -> Steps<Take, Error> {
    let pixel = Viewport::new(scene::screen(), scene::density()).pixel();
    let mut steps = Steps::default();
    steps.push(Take::Sequence("show"), move |view| {
        tap_field_end(view, pixel)
    });
    let typed = demo.clone();
    steps.push(Take::Unrecorded, move |view| {
        (QUERY.chars()).try_for_each(|character| tap(view, &stage::key_name(&typed, character)))
    });
    let shown = demo.clone();
    steps.push(Take::Sequence("hide"), move |view| {
        let outside = outside_field(view, &shown)?;
        stage::tap_at(view, outside);
        Ok(())
    });
    steps.push(Take::Unrecorded, move |view| tap_field_end(view, pixel));
    previews(&mut steps);
    for placement in [docked(Side::Left), Placement::FullWidth] {
        let placed = demo.clone();
        steps.push(Take::Sequence("dock"), move |_| {
            placed.placement.set(placement);
            Ok(())
        });
    }
    steps.push(Take::Unrecorded, |view| tap(view, copy(CopyKey::Back)));
    steps.push(Take::Still("reduced-motion-before"), |view| {
        view.provide(&REDUCED_MOTION, true);
        Ok(())
    });
    steps.push(Take::Still("reduced-motion-after"), move |view| {
        tap_field_end(view, pixel)
    });
    steps
}

/// A key preview for one finger, then previews for a finger that lifts while the next one touches down.
fn previews(steps: &mut Steps<Take, Error>) {
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
        steps.push(take, move |view| {
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
                view.input(InputEvent::TouchDown(finger(touch(point), at)));
            }
            Finger::Up(point) => {
                if let Some(at) = self.0.borrow_mut().remove(&point) {
                    view.input(InputEvent::TouchUp(finger(touch(point), at)));
                }
            }
        }
        Ok(())
    }
}

fn finger(id: TouchId, at: Point) -> TouchSample {
    TouchSample {
        id,
        at,
        kind: PointerKind::Finger,
    }
}

fn tap(view: &mut View, name: &str) -> Result<(), Error> {
    let at = stage::key(view, name)?;
    stage::tap_at(view, at);
    Ok(())
}

fn tap_field_end(view: &mut View, pixel: Dp) -> Result<(), Error> {
    let at = stage::field_end(view, pixel)?;
    stage::tap_at(view, at);
    Ok(())
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
