// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    model::Demo,
    scene::{FIELD, FIELD_TEXT},
};
use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
use lavashell_example_support::node;

pub const QUERY: &str = "mara vale glasshouse live at the harbour sessions extended";

#[must_use]
pub fn touch(point: i32) -> TouchId {
    TouchId { seat: 0, point }
}

#[must_use]
pub fn finger(point: i32, at: Point) -> TouchSample {
    TouchSample::finger(touch(point), at)
}

pub fn panel<'v>(view: &'v View, demo: &Demo) -> Result<&'v AccessibleNode, Error> {
    let name = (demo.settings).with(|settings| settings.text.keyboard.clone());
    Ok(node(view, &Role::Group, &name)?)
}

pub fn key_bounds(view: &View, name: &str) -> Result<Rect, Error> {
    Ok(node(view, &Role::Button, name)?.bounds)
}

pub fn key(view: &View, name: &str) -> Result<Point, Error> {
    Ok(key_bounds(view, name)?.center())
}

#[must_use]
pub fn key_name(demo: &Demo, character: char) -> String {
    match character {
        ' ' => (demo.settings).with(|settings| settings.text.space.clone()),
        letter => letter.to_string(),
    }
}

pub fn tap_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

pub fn tap(view: &mut View, name: &str) -> Result<(), Error> {
    let at = key(view, name)?;
    tap_at(view, at);
    Ok(())
}

pub fn tap_field_end(view: &mut View, pixel: Dp) -> Result<(), Error> {
    let at = field_end(view, pixel)?;
    tap_at(view, at);
    Ok(())
}

pub fn field_end(view: &View, pixel: Dp) -> Result<Point, Error> {
    let field = (view.bounds_of(&FIELD)).ok_or(Error::MissingField)?;
    let text = field.inset(FIELD_TEXT);
    Ok(Point::new(text.x + text.width - pixel, text.center().y))
}
