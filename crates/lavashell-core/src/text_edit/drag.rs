// SPDX-License-Identifier: GPL-3.0-only
use super::{Dp, Point, Range, TextCaret, TextEdit, TextGeometry};

#[derive(Clone)]
pub(super) struct SelectionDrag {
    source: String,
    range: Range<usize>,
    press: Point,
    click: TextCaret,
    destination: TextCaret,
    moving: bool,
}

impl TextEdit {
    /// Whether a press inside the selection has turned into a drag of the selected text. That is the case once the
    /// pointer, now at `at`, has moved at least `slop` away from the point of the press since the press.
    pub fn drag_ready(&self, at: Point, slop: Dp) -> bool {
        self.drag.as_ref().is_some_and(|drag| {
            drag.moving || (at.x - drag.press.x).hypot(at.y - drag.press.y) >= slop
        })
    }

    pub fn press(&mut self, value: &str, geometry: &TextGeometry, at: Point, extend: bool) {
        self.clamp_shaped(value, geometry);
        let click = geometry.hit_caret(at);
        let range = self.selection();
        if !extend
            && geometry
                .selection(range.clone())
                .iter()
                .any(|r| r.contains(at))
        {
            self.drag = Some(SelectionDrag {
                source: value.to_string(),
                range,
                press: at,
                click,
                destination: click,
                moving: false,
            });
            return;
        }
        self.drag = None;
        self.move_to(click.index, extend);
        self.line = Some(click.line);
        self.goal_x = None;
    }

    pub fn drag_to(&mut self, geometry: &TextGeometry, at: Point, slop: Dp) {
        let ready = self.drag_ready(at, slop);
        if let Some(drag) = &mut self.drag {
            drag.moving = ready;
            drag.destination = geometry.hit_caret(at);
        } else {
            let caret = geometry.hit_caret(at);
            self.move_to(caret.index, true);
            self.line = Some(caret.line);
            self.goal_x = None;
        }
    }

    pub fn drop_caret(&self) -> Option<TextCaret> {
        self.drag
            .as_ref()
            .filter(|d| d.moving)
            .map(|d| d.destination)
    }

    pub fn release(&mut self, value: &mut String, allow_move: bool, inside: bool) {
        let Some(drag) = self.drag.take() else { return };
        if !inside || *value != drag.source {
            return;
        }
        if !drag.moving {
            self.move_to(drag.click.index, false);
            self.line = Some(drag.click.line);
        } else if allow_move {
            self.move_range(value, drag.range, drag.destination.index);
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    pub fn clamp_shaped(&mut self, value: &str, geometry: &TextGeometry) {
        self.clamp(value);
        self.cursor = geometry.boundary(self.cursor);
        self.anchor = self.anchor.map(|index| geometry.boundary(index));
        if let Some(drag) = &mut self.drag {
            let destination = drag.destination;
            drag.destination = geometry.caret_on_line(destination.index, Some(destination.line));
        }
    }

    pub(crate) fn move_range(
        &mut self,
        value: &mut String,
        range: Range<usize>,
        destination: usize,
    ) {
        if (range.start..=range.end).contains(&destination) {
            return;
        }
        let text = value[range.clone()].to_string();
        let destination = if destination > range.end {
            destination - range.len()
        } else {
            destination
        };
        value.replace_range(range, "");
        value.insert_str(destination, &text);
        self.anchor = Some(destination);
        self.cursor = destination + text.len();
        self.line = None;
        self.goal_x = None;
    }
}
