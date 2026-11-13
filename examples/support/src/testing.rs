// SPDX-License-Identifier: GPL-3.0-only
use crate::error::reject_failures;
use crate::{Error, fonts, write_capture};
use lavashell::prelude::*;
use lavashell::renderer::text::{Shaper, TextRendering};
use lavashell_test_platform::RecordedFrame;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{cell::RefCell, rc::Rc};

mod steps;

pub use steps::KeyboardKey;
pub(crate) use steps::press;

/// The shaper that measures the text of the views of a test, with the example fonts. A test creates it once and
/// shares it among its steps.
#[derive(Clone)]
pub struct Shaping(Rc<RefCell<Shaper>>);

impl Shaping {
    /// # Errors
    /// Returns an error when the example fonts do not load or the shaper cannot be created.
    pub fn new() -> Result<Self, Error> {
        let shaper = Shaper::with_rendering(&fonts()?, TextRendering::default());
        Ok(Self(Rc::new(RefCell::new(shaper.map_err(Error::Shaper)?))))
    }

    /// A snapshot of the scene in a frame of `view`, taken in `viewport` at the time of the view. The view frames
    /// again into the scene of its surface afterwards, so the surface shows every change made before the snapshot.
    ///
    /// # Errors
    /// Returns an error when the frame cannot be shown or a component failed in it.
    pub fn scene(&self, view: &mut View, viewport: Viewport) -> Result<SceneSnapshot, Error> {
        let mut builder = SceneBuilder::default();
        let framed = view.frame_into(viewport, &mut *self.0.borrow_mut(), &mut builder);
        view.invalidate();
        reject_failures(framed.map_err(Error::Frame)?)?;
        Ok(builder.snapshot())
    }
}

/// The path of the capture `name` in `directory`.
#[must_use]
pub fn capture_path(directory: impl AsRef<Path>, name: &str) -> PathBuf {
    directory.as_ref().join(format!("{name}.png"))
}

/// Writes the first of the frames labeled with each path to that path, as [`write_capture`] does.
///
/// # Errors
/// Returns an error when a file cannot be written.
pub fn write_first_frames(frames: Vec<(PathBuf, RecordedFrame)>) -> Result<(), Error> {
    let mut first = BTreeMap::new();
    for (path, frame) in frames {
        let _first_frame_kept = first.entry(path).or_insert(frame.png);
    }
    (first.into_iter()).try_for_each(|(path, png)| write_capture(path, &png))
}

/// # Errors
/// Returns [`Error::MissingControl`] when no node of `view` is tagged `tag`.
pub fn control(view: &View, tag: &str) -> Result<Rect, Error> {
    view.bounds_of(tag)
        .ok_or_else(|| Error::MissingControl(tag.to_owned()))
}

/// # Errors
/// Returns [`Error::MissingControl`] when no node of `view` is tagged `tag`.
pub fn interaction(view: &View, tag: &str) -> Result<InteractionState, Error> {
    view.interaction_state(tag)
        .ok_or_else(|| Error::MissingControl(tag.to_owned()))
}

/// # Errors
/// Returns [`Error::MissingControl`] when no node of `view` is tagged `tag`.
pub fn click(view: &mut View, tag: &str) -> Result<(), Error> {
    let at = control(view, tag)?.center();
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
    Ok(())
}

pub fn drag(view: &mut View, start: Point, end: Point) {
    view.input(InputEvent::Press(start));
    view.input(InputEvent::Move(end));
    view.input(InputEvent::Release(end));
}
