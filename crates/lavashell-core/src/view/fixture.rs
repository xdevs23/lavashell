// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Point;
use crate::view::{InputEvent, View};

pub fn press_and_release_at(view: &mut View, at: Point) {
    view.input(InputEvent::Press(at));
    view.input(InputEvent::Release(at));
}
