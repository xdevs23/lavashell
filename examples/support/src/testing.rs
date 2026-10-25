// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;

/// A new tree that shows the whole scene of a frame of `view`.
///
/// # Errors
/// Returns an error when the frame cannot be shown.
pub fn scene(
    view: &mut View,
    viewport: Viewport,
    text: &mut impl TextMeasurer,
) -> Result<SceneTree, Error> {
    let mut tree = SceneTree::default();
    (view.show_frame(&mut tree, viewport, text)).map_err(Error::Frame)?;
    Ok(tree)
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
/// Returns [`Error::MissingNode`] when the accessibility tree of `view` contains no node with the role `role` and
/// the name `name`.
pub fn node<'a>(view: &'a View, role: &Role, name: &str) -> Result<&'a AccessibleNode, Error> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
        .ok_or_else(|| Error::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
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
