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

    pub fn settle(&mut self) -> Result<(), Error> {
        Ok(self.offscreen.settle()?)
    }

    pub fn run_until(&mut self, done: impl Fn(&Self) -> bool) -> Result<(), Error> {
        while !done(self) {
            self.offscreen.advance()?;
        }
        Ok(())
    }

    pub fn input(&mut self, input: InputEvent) -> Result<(), Error> {
        self.offscreen.view.input(input);
        Ok(self.offscreen.frame()?)
    }

    #[must_use]
    pub fn node(&self, role: &Role, name: &str) -> Option<&AccessibleNode> {
        node(&self.offscreen.view, role, name)
    }

    pub fn find(&self, role: &Role, name: &str) -> Result<&AccessibleNode, Error> {
        find(&self.offscreen.view, role, name)
    }

    pub fn panel(&self) -> Result<&AccessibleNode, Error> {
        panel(&self.offscreen.view, &self.demo)
    }

    #[must_use]
    pub fn shown(&self) -> bool {
        self.panel().is_ok()
    }

    pub fn key_bounds(&self, name: &str) -> Result<Rect, Error> {
        key_bounds(&self.offscreen.view, name)
    }

    pub fn key(&self, name: &str) -> Result<Point, Error> {
        key(&self.offscreen.view, name)
    }

    pub fn tap_at(&mut self, at: Point) -> Result<(), Error> {
        tap_at(&mut self.offscreen.view, at);
        Ok(self.offscreen.frame()?)
    }

    pub fn tap(&mut self, name: &str) -> Result<(), Error> {
        self.tap_at(self.key(name)?)
    }

    pub fn type_text(&mut self, text: &str) -> Result<(), Error> {
        for character in text.chars() {
            self.tap(&key_name(&self.demo, character))?;
        }
        Ok(())
    }

    pub fn field_end(&self) -> Result<Point, Error> {
        field_end(&self.offscreen.view, self.offscreen.viewport().pixel())
    }

    pub fn focus_field(&mut self) -> Result<(), Error> {
        self.tap_at(self.field_end()?)
    }
}

#[must_use]
fn node<'v>(view: &'v View, role: &Role, name: &str) -> Option<&'v AccessibleNode> {
    (view.accessibility().nodes.values())
        .find(|node| node.role == *role && node.name.as_deref() == Some(name))
}

fn find<'v>(view: &'v View, role: &Role, name: &str) -> Result<&'v AccessibleNode, Error> {
    node(view, role, name).ok_or_else(|| {
        Error::Example(ExampleError::MissingNode {
            role: role.clone(),
            name: name.to_owned(),
        })
    })
}

pub(crate) fn panel<'v>(view: &'v View, demo: &Demo) -> Result<&'v AccessibleNode, Error> {
    let name = (demo.settings).with(|settings| settings.text.keyboard.clone());
    find(view, &Role::Group, &name)
}

fn key_bounds(view: &View, name: &str) -> Result<Rect, Error> {
    Ok(find(view, &Role::Button, name)?.bounds)
}

pub(crate) fn key(view: &View, name: &str) -> Result<Point, Error> {
    Ok(key_bounds(view, name)?.center())
}

#[must_use]
pub(crate) fn key_name(demo: &Demo, character: char) -> String {
    match character {
        ' ' => (demo.settings).with(|settings| settings.text.space.clone()),
        letter => letter.to_string(),
    }
}

pub(crate) fn tap_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}

pub(crate) fn field_end(view: &View, pixel: Dp) -> Result<Point, Error> {
    let field = (view.bounds_of(&FIELD)).ok_or(Error::MissingField)?;
    let text = field.inset(FIELD_TEXT);
    Ok(Point::new(text.x + text.width - pixel, text.center().y))
}
