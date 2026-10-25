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
        (self.offscreen.view.accessibility().nodes.values())
            .find(|node| node.role == *role && node.name.as_deref() == Some(name))
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no such node.
    pub fn find(&self, role: &Role, name: &str) -> Result<&AccessibleNode, Error> {
        self.node(role, name).ok_or_else(|| {
            Error::Example(ExampleError::MissingNode {
                role: role.clone(),
                name: name.to_owned(),
            })
        })
    }

    /// The accessibility node of the keyboard panel, found by the panel name set in the keyboard settings.
    ///
    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no keyboard panel.
    pub fn panel(&self) -> Result<&AccessibleNode, Error> {
        let name = (self.demo.settings).with(|settings| settings.text.keyboard.clone());
        self.find(&Role::Group, &name)
    }

    #[must_use]
    pub fn shown(&self) -> bool {
        self.panel().is_ok()
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no key named `name`.
    pub fn key_bounds(&self, name: &str) -> Result<Rect, Error> {
        Ok(self.find(&Role::Button, name)?.bounds)
    }

    /// # Errors
    /// Returns [`ExampleError::MissingNode`] when the view shows no key named `name`.
    pub fn key(&self, name: &str) -> Result<Point, Error> {
        Ok(self.key_bounds(name)?.center())
    }

    /// # Errors
    /// Returns an error when the frame after the tap cannot be shown.
    pub fn tap_at(&mut self, at: Point) -> Result<(), Error> {
        self.offscreen.view.input(InputEvent::Press(at));
        self.input(InputEvent::Release(at))
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
            let name = match character {
                ' ' => (self.demo.settings).with(|settings| settings.text.space.clone()),
                letter => letter.to_string(),
            };
            self.tap(&name)?;
        }
        Ok(())
    }

    /// The point one device pixel inside the right end of the text area in the search field, centered
    /// vertically. A press there puts the caret after the query.
    ///
    /// # Errors
    /// Returns [`Error::MissingField`] when the scene has no search field.
    pub fn field_end(&self) -> Result<Point, Error> {
        let field = (self.offscreen.view.bounds_of(FIELD)).ok_or(Error::MissingField)?;
        let text = field.inset(FIELD_TEXT);
        let pixel = self.offscreen.viewport().pixel();
        Ok(Point::new(text.x + text.width - pixel, text.center().y))
    }

    /// # Errors
    /// Returns an error when the scene has no search field or the frame cannot be shown.
    pub fn focus_field(&mut self) -> Result<(), Error> {
        self.tap_at(self.field_end()?)
    }
}
