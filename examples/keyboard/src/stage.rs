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

/// The touch sample of the finger with touch point `point` at `at`.
#[must_use]
pub fn finger(point: i32, at: Point) -> TouchSample {
    TouchSample {
        id: touch(point),
        at,
        kind: PointerKind::Finger,
    }
}

/// The accessibility node of the keyboard panel of `demo` in `view`, found by the panel name set in the keyboard
/// settings.
///
/// # Errors
/// Returns [`lavashell_example_support::Error::MissingNode`] when `view` shows no keyboard panel.
pub fn panel<'v>(view: &'v View, demo: &Demo) -> Result<&'v AccessibleNode, Error> {
    let name = (demo.settings).with(|settings| settings.text.keyboard.clone());
    Ok(node(view, &Role::Group, &name)?)
}

/// The bounds of the key named `name` in `view`.
///
/// # Errors
/// Returns [`lavashell_example_support::Error::MissingNode`] when `view` shows no key named `name`.
pub fn key_bounds(view: &View, name: &str) -> Result<Rect, Error> {
    Ok(node(view, &Role::Button, name)?.bounds)
}

/// The center of the key named `name` in `view`.
///
/// # Errors
/// Returns [`lavashell_example_support::Error::MissingNode`] when `view` shows no key named `name`.
pub fn key(view: &View, name: &str) -> Result<Point, Error> {
    Ok(key_bounds(view, name)?.center())
}

/// The name of the key of `demo` that types `character`.
#[must_use]
pub fn key_name(demo: &Demo, character: char) -> String {
    match character {
        ' ' => (demo.settings).with(|settings| settings.text.space.clone()),
        letter => letter.to_string(),
    }
}

/// Presses and releases at `at`, as one tap.
pub fn tap_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

/// Taps the key named `name` in `view`.
///
/// # Errors
/// Returns [`lavashell_example_support::Error::MissingNode`] when `view` shows no key named `name`.
pub fn tap(view: &mut View, name: &str) -> Result<(), Error> {
    let at = key(view, name)?;
    tap_at(view, at);
    Ok(())
}

/// Taps the search field in `view`, `pixel` inside the right end of the text area. The caret lands after the query.
///
/// # Errors
/// Returns [`Error::MissingField`] when the scene has no search field.
pub fn tap_field_end(view: &mut View, pixel: Dp) -> Result<(), Error> {
    let at = field_end(view, pixel)?;
    tap_at(view, at);
    Ok(())
}

/// The point `pixel` inside the right end of the text area in the search field of `view`, centered vertically. A
/// press there puts the caret after the query.
///
/// # Errors
/// Returns [`Error::MissingField`] when the scene has no search field.
pub fn field_end(view: &View, pixel: Dp) -> Result<Point, Error> {
    let field = (view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
    let text = field.inset(FIELD_TEXT);
    Ok(Point::new(text.x + text.width - pixel, text.center().y))
}
