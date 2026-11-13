// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    model::Demo,
    scene::{self, FIELD, FIELD_TEXT},
};
use lavashell::core::accessibility::AccessibleNode;
use lavashell::prelude::*;
use lavashell_example_support::{Error as ExampleError, OffscreenView};

pub const QUERY: &str = "mara vale glasshouse live at the harbour sessions extended";

#[must_use]
pub fn touch(point: i32) -> TouchId {
    TouchId { seat: 0, point }
}

pub struct Stage {
    pub demo: Demo,
    pub offscreen: OffscreenView,
}

impl Stage {
    /// A stage that shows `demo` on the car screen, at rest.
    ///
    /// # Errors
    /// Returns an error when the offscreen view cannot be created or framed.
    pub fn new(demo: Demo) -> Result<Self, Error> {
        let composed = demo.clone();
        let view = View::new(lavashell_example_support::platform(), move |ui| {
            scene::compose(ui, &composed);
        });
        let mut stage = Self {
            demo,
            offscreen: OffscreenView::new(view, Some(scene::screen()), scene::density())?,
        };
        stage.settle()?;
        Ok(stage)
    }

    /// Frames the view until every motion of the scene came to rest.
    ///
    /// # Errors
    /// Returns an error when a motion runs without a deadline or a frame cannot be shown.
    pub fn settle(&mut self) -> Result<(), Error> {
        Ok(self.offscreen.settle()?)
    }

    /// Frames the view at each of its deadlines until `done` returns true.
    ///
    /// # Errors
    /// Returns an error when the view has no deadline left before `done` returns true, or a frame
    /// cannot be shown.
    pub fn run_until(&mut self, done: impl Fn(&Self) -> bool) -> Result<(), Error> {
        while !done(self) {
            self.offscreen.advance()?;
        }
        Ok(())
    }

    /// # Errors
    /// Returns an error when the frame after the input cannot be shown.
    pub fn input(&mut self, input: InputEvent) -> Result<(), Error> {
        self.offscreen.view.input(input);
        Ok(self.offscreen.frame()?)
    }

    #[must_use]
    pub fn node(&self, role: &Role, name: &str) -> Option<&AccessibleNode> {
        node(&self.offscreen.view, role, name)
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no such node.
    pub fn find(&self, role: &Role, name: &str) -> Result<&AccessibleNode, Error> {
        find(&self.offscreen.view, role, name)
    }

    /// The accessibility node of the keyboard panel, found by the panel name set in the keyboard settings.
    ///
    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no keyboard panel.
    pub fn panel(&self) -> Result<&AccessibleNode, Error> {
        panel(&self.offscreen.view, &self.demo)
    }

    #[must_use]
    pub fn shown(&self) -> bool {
        self.panel().is_ok()
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no key named `name`.
    pub fn key_bounds(&self, name: &str) -> Result<Rect, Error> {
        key_bounds(&self.offscreen.view, name)
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no key named `name`.
    pub fn key(&self, name: &str) -> Result<Point, Error> {
        key(&self.offscreen.view, name)
    }

    /// # Errors
    /// Returns an error when the frame after the tap cannot be shown.
    pub fn tap_at(&mut self, at: Point) -> Result<(), Error> {
        tap_at(&mut self.offscreen.view, at);
        Ok(self.offscreen.frame()?)
    }

    /// # Errors
    /// Returns an error when the view shows no key named `name` or the frame cannot be shown.
    pub fn tap(&mut self, name: &str) -> Result<(), Error> {
        self.tap_at(self.key(name)?)
    }

    /// Taps the key of each character of `text`.
    ///
    /// # Errors
    /// Returns an error when the view shows no key for a character or a frame cannot be shown.
    pub fn type_text(&mut self, text: &str) -> Result<(), Error> {
        for character in text.chars() {
            self.tap(&key_name(&self.demo, character))?;
        }
        Ok(())
    }

    /// The point one device pixel inside the right end of the text area in the search field, centered
    /// vertically. A press there puts the caret after the query.
    ///
    /// # Errors
    /// Returns [`Error::MissingField`] when the scene has no search field.
    pub fn field_end(&self) -> Result<Point, Error> {
        field_end(&self.offscreen.view, self.offscreen.viewport().pixel())
    }

    /// # Errors
    /// Returns an error when the scene has no search field or the frame cannot be shown.
    pub fn focus_field(&mut self) -> Result<(), Error> {
        self.tap_at(self.field_end()?)
    }
}

/// The accessibility node of `view` with `role` and `name`.
#[must_use]
fn node<'v>(view: &'v View, role: &Role, name: &str) -> Option<&'v AccessibleNode> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
}

/// # Errors
/// Returns [`ExampleError::MissingNode`] when `view` shows no such node.
fn find<'v>(view: &'v View, role: &Role, name: &str) -> Result<&'v AccessibleNode, Error> {
    node(view, role, name).ok_or_else(|| {
        Error::Example(ExampleError::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
    })
}

/// The accessibility node of the keyboard panel of `demo` in `view`, found by the panel name set in the keyboard
/// settings.
///
/// # Errors
/// Returns [`ExampleError::MissingNode`] when `view` shows no keyboard panel.
pub(crate) fn panel<'v>(view: &'v View, demo: &Demo) -> Result<&'v AccessibleNode, Error> {
    let name = (demo.settings).with(|settings| settings.text.keyboard.clone());
    find(view, &Role::Group, &name)
}

/// The bounds of the key named `name` in `view`.
///
/// # Errors
/// Returns [`ExampleError::MissingNode`] when `view` shows no key named `name`.
fn key_bounds(view: &View, name: &str) -> Result<Rect, Error> {
    Ok(find(view, &Role::Button, name)?.bounds)
}

/// The center of the key named `name` in `view`.
///
/// # Errors
/// Returns [`ExampleError::MissingNode`] when `view` shows no key named `name`.
pub(crate) fn key(view: &View, name: &str) -> Result<Point, Error> {
    Ok(key_bounds(view, name)?.center())
}

/// The name of the key of `demo` that types `character`.
#[must_use]
pub(crate) fn key_name(demo: &Demo, character: char) -> String {
    match character {
        ' ' => (demo.settings).with(|settings| settings.text.space.clone()),
        letter => letter.to_string(),
    }
}

/// Presses and releases at `at`, as one tap.
pub(crate) fn tap_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

/// The point `pixel` inside the right end of the text area in the search field of `view`, centered vertically. A
/// press there puts the caret after the query.
///
/// # Errors
/// Returns [`Error::MissingField`] when the scene has no search field.
pub(crate) fn field_end(view: &View, pixel: Dp) -> Result<Point, Error> {
    let field = (view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
    let text = field.inset(FIELD_TEXT);
    Ok(Point::new(text.x + text.width - pixel, text.center().y))
}
