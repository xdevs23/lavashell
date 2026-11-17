// SPDX-License-Identifier: GPL-3.0-only
use crate::error::reject_failures;
use crate::{Error, fonts, write_capture};
use lavashell::core::ui::{NodeTag, ViewTag};
use lavashell::prelude::*;
use lavashell::renderer::text::{Shaper, TextRendering};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{cell::RefCell, rc::Rc};

mod steps;

pub use lavashell_test_platform::RecordedFrame;

#[derive(Clone)]
pub struct Shaping(Rc<RefCell<Shaper>>);

impl Shaping {
    pub fn new() -> Result<Self, Error> {
        let shaper = Shaper::with_rendering(&fonts()?, TextRendering::default());
        Ok(Self(Rc::new(RefCell::new(shaper.map_err(Error::Shaper)?))))
    }

    pub fn scene(&self, view: &mut View, viewport: Viewport) -> Result<SceneSnapshot, Error> {
        let probed = view.probe_scene(viewport, &mut *self.0.borrow_mut());
        let (scene, failures) = probed.map_err(Error::Frame)?;
        reject_failures(failures)?;
        Ok(scene)
    }
}

#[must_use]
pub fn capture_path(directory: impl AsRef<Path>, name: &str) -> PathBuf {
    directory.as_ref().join(format!("{name}.png"))
}

pub fn write_first_frames(frames: Vec<(PathBuf, RecordedFrame)>) -> Result<(), Error> {
    let mut first = BTreeMap::new();
    for (path, frame) in frames {
        let _first_frame_kept = first.entry(path).or_insert(frame.png);
    }
    (first.into_iter()).try_for_each(|(path, png)| write_capture(path, &png))
}

pub fn control(view: &View, tag: impl ViewTag) -> Result<Rect, Error> {
    view.bounds_of(&tag)
        .ok_or_else(|| Error::MissingControl(NodeTag::new(tag)))
}

pub fn interaction(view: &View, tag: impl ViewTag) -> Result<InteractionState, Error> {
    view.interaction_state(&tag)
        .ok_or_else(|| Error::MissingControl(NodeTag::new(tag)))
}

pub fn click(view: &mut View, tag: impl ViewTag) -> Result<(), Error> {
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
