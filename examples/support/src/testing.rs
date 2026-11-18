// SPDX-License-Identifier: GPL-3.0-only
use crate::error::reject_failures;
use crate::{Error, fonts};
use lavashell::core::ui::{NodeTag, ViewTag};
use lavashell::core::view::fixture::press_and_release_at;
use lavashell::prelude::*;
use lavashell::renderer::text::{Shaper, TextRendering};
use std::{cell::RefCell, rc::Rc};

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
    press_and_release_at(view, at);
    Ok(())
}

pub fn drag(view: &mut View, start: Point, end: Point) {
    view.input(InputEvent::Press(start));
    view.input(InputEvent::Move(end));
    view.input(InputEvent::Release(end));
}
