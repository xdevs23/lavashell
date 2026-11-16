// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, without_failures};
use lavashell::core::accessibility::AccessibleNode;
use lavashell::core::ui::{NodeTag, ViewTag};
use lavashell::prelude::*;

pub fn scene(
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<SceneSnapshot, Error> {
    let mut builder = SceneBuilder::default();
    without_failures(view.frame_into(viewport, text, &mut builder))?;
    Ok(builder.snapshot())
}

pub fn control(view: &View, tag: impl ViewTag) -> Result<Rect, Error> {
    view.bounds_of(&tag)
        .ok_or_else(|| Error::MissingControl(NodeTag::new(tag)))
}

pub fn interaction(view: &View, tag: impl ViewTag) -> Result<InteractionState, Error> {
    view.interaction_state(&tag)
        .ok_or_else(|| Error::MissingControl(NodeTag::new(tag)))
}

pub fn node<'a>(view: &'a View, role: &Role, name: &str) -> Result<&'a AccessibleNode, Error> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
        .ok_or_else(|| Error::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
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
