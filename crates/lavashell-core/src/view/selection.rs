// SPDX-License-Identifier: GPL-3.0-only
use super::{BTreeSet, FieldGeometry, Hit, NodeId, NodeKind, Phase, Point, TextEdit, View};
use crate::ui::{FieldContent, Lines};
use std::borrow::Cow;

mod source;

pub use source::TextSelectionSource;

mod paste;

pub use paste::TextPasteTarget;

/// The point at which dropped text goes into a text field: the field, the text shown by the field when the target was
/// found, and the offset of the insertion point in that text.
#[derive(Clone)]
pub struct TextDropTarget {
    id: NodeId,
    value: String,
    offset: usize,
}

impl TextDropTarget {
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }
}

impl View {
    /// The id of the focused editable field, unless that field is protected. A host reads this id before it requests
    /// the content of the clipboard, because the content arrives later and the focus can move in between.
    #[must_use]
    pub fn editing_target(&self) -> Option<NodeId> {
        (self.session_field())
            .filter(|index| !self.protected_index(*index))
            .map(|index| self.nodes[index].id)
    }

    pub fn copy_selection(&mut self, cut: bool) -> Option<String> {
        let id = self.selection_target()?;
        let index = self.nodes.iter().position(|node| node.id == id)?;
        if self.protected_index(index) {
            return None;
        }
        let value = self.text_value(index)?.into_owned();
        let editing = self.edits.get_mut(&id)?;
        editing.clamp(&value);
        let selection = editing.selection();
        if selection.is_empty() {
            return None;
        }
        let text = value[selection].to_string();
        if cut {
            self.edit(|edit, value| edit.replace(value, ""));
        }
        Some(text)
    }

    /// The drop target at `at` in an editable text field, found from the text geometry of the last paint, whatever
    /// node has the focus.
    #[must_use]
    pub fn text_drop_target(&self, at: Point) -> Option<TextDropTarget> {
        let hit = self.hit_at(at)?;
        let index = hit.input?;
        if !self.writable_text_target(index) {
            return None;
        }
        let value = self.text_value(index)?;
        let positions = self
            .text_positions
            .get(&hit.id)
            .filter(|p| p.value == *value)?;
        let point = self.local_point(hit, positions, at)?;
        Some(TextDropTarget {
            id: hit.id,
            value: value.to_string(),
            offset: positions.geometry.hit(point),
        })
    }

    /// Inserts `text` at `target` and returns true when the field still shows the text recorded in `target` and can
    /// still be edited. Returns false and changes nothing otherwise.
    pub fn insert_text_at(&mut self, target: &TextDropTarget, text: &str) -> bool {
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
        let Some(index) = self.nodes.iter().position(|node| node.id == target.id) else {
            return false;
        };
        let insertion = target.offset..target.offset;
        if !self.writable_text_target(index)
            || self.text_value(index).as_deref() != Some(target.value.as_str())
            || !(self.text_positions.get(&target.id))
                .is_some_and(|p| p.shaped_range(&target.value, &insertion))
        {
            return false;
        }
        let text = self.normalize_for_field(index, text);
        self.edit_at(index, |edit, value| {
            edit.move_to(target.offset, false);
            edit.replace(value, &text);
        });
        true
    }

    pub(super) fn text_selectable(&self, mut index: usize) -> bool {
        if matches!(self.nodes[index].kind, NodeKind::TextInput { .. }) {
            return true;
        }
        if !matches!(self.nodes[index].kind, NodeKind::Text(..)) {
            return false;
        }
        loop {
            if let Some(enabled) = self.nodes[index].user_select {
                return enabled;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return false;
            };
            index = parent;
        }
    }

    /// Whether the node at `index` is a writable text field, which a transfer can edit now.
    fn writable_text_target(&self, index: usize) -> bool {
        !self.protected_index(index)
            && self.current_text_target(index)
            && matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
            && !self.accessibility_semantics(index).state.form.read_only
    }

    /// `text` with the line breaks that the field at `index` accepts. A field of several lines leaves line feeds as
    /// they are. A field of one line turns them into spaces.
    pub(super) fn normalize_for_field(&self, index: usize, text: &str) -> String {
        if matches!(
            &self.nodes[index].kind,
            NodeKind::TextInput { content, .. } if content.lines() == Lines::Several
        ) {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.replace(['\r', '\n'], " ")
        }
    }

    fn current_text_target(&self, mut index: usize) -> bool {
        if self.scope.dirty().contains(Phase::Composition) || self.input_disabled(index) {
            return false;
        }
        loop {
            if self.nodes[index]
                .component
                .as_ref()
                .is_some_and(|c| c.scope.dirty().contains(Phase::Composition))
            {
                return false;
            }
            let Some(parent) = self.parents.node_of(index) else {
                return true;
            };
            index = parent;
        }
    }

    pub(super) fn selection_target(&self) -> Option<NodeId> {
        self.focus.filter(|id| {
            self.hits
                .iter()
                .any(|hit| hit.id == *id && hit.input.is_some())
        })
    }

    /// The text that the node at `index` shows, where it is a text or a field.
    pub(super) fn text_value(&self, index: usize) -> Option<Cow<'_, str>> {
        match &self.nodes[index].kind {
            NodeKind::Text(value, _) => Some(Cow::Borrowed(value)),
            NodeKind::TextInput { content, .. } => Some(content.shown()),
            _ => None,
        }
    }

    pub(super) fn edit(&mut self, update: impl FnOnce(&mut TextEdit, &mut String)) {
        if let Some(focused) = self.focused_text() {
            self.edit_at(focused.index, update);
        }
    }

    fn edit_at(&mut self, index: usize, update: impl FnOnce(&mut TextEdit, &mut String)) {
        let Some(before) = self.text_value(index).map(Cow::into_owned) else {
            return;
        };
        let id = self.nodes[index].id;
        let read_only = self.protected_index(index)
            || matches!(self.nodes[index].kind, NodeKind::Text(..))
            || self.accessibility_semantics(index).state.form.read_only;
        let edit = self
            .edits
            .entry(id)
            .or_insert_with(|| TextEdit::at_end(&before));
        let before_edit = edit.clone();
        let mut value = before.clone();
        update(edit, &mut value);
        if read_only && value != before {
            *edit = before_edit;
        } else if value != before
            && let NodeKind::TextInput {
                content:
                    FieldContent::Plain {
                        value: current,
                        change,
                        ..
                    },
                ..
            } = &mut self.nodes[index].kind
        {
            current.clone_from(&value);
            let change = change.clone();
            change.borrow_mut()(value);
        }
        self.redraw = true;
    }

    /// Places the caret of the field `capture` at `at`, or extends the selection of that field to `at` when `extend`
    /// is true.
    pub(super) fn select_at(&mut self, capture: Option<NodeId>, at: Point, extend: bool) {
        let Some((hit, index)) = capture.and_then(|id| input_hit(&self.hits, id)) else {
            return;
        };
        let Some(positions) = self.text_positions.get(&hit.id) else {
            return;
        };
        if self.text_value(index).as_deref() != Some(positions.value.as_str()) {
            return;
        }
        let Some(point) = self.local_point(hit, positions, at) else {
            return;
        };
        let slop = self.touch_slop();
        let edit = self.edits.entry(hit.id).or_default();
        if extend {
            edit.drag_to(&positions.geometry, point, slop);
        } else {
            edit.press(
                &positions.value,
                &positions.geometry,
                point,
                self.modifiers.shift,
            );
        }
        self.redraw = true;
    }

    pub(super) fn release_text(&mut self, capture: Option<NodeId>, at: Point) {
        let Some(id) = capture else { return };
        let Some((hit, index)) = input_hit(&self.hits, id) else {
            if let Some(edit) = self.edits.get_mut(&id) {
                edit.cancel_drag();
            }
            return;
        };
        if !self.current_text_target(index) {
            if let Some(edit) = self.edits.get_mut(&hit.id) {
                edit.cancel_drag();
            }
            return;
        }
        let inside = hit.contains(at, &self.moving);
        let allow_move = matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
            && !self.accessibility_semantics(index).state.form.read_only;
        self.select_at(capture, at, true);
        self.edit_at(index, |edit, value| edit.release(value, allow_move, inside));
    }

    /// Whether a move of selected text is in progress in any field.
    pub(super) fn text_drag_in_progress(&self) -> bool {
        self.edits.values().any(TextEdit::is_dragging)
    }

    /// Cancels every move of selected text, and ends the pointer captures of the fields in which the text was moving.
    pub(super) fn cancel_text_drag(&mut self) {
        let cancelled: BTreeSet<_> = (self.edits.iter_mut())
            .filter(|(_, edit)| edit.is_dragging())
            .map(|(id, edit)| {
                edit.cancel_drag();
                *id
            })
            .collect();
        if cancelled.is_empty() {
            return;
        }
        self.end_captures(|id| cancelled.contains(&id));
        self.redraw = true;
    }
}

/// The hit of the text field `id`, with the index of the node that receives the text input of that hit.
fn input_hit(hits: &[Hit], id: NodeId) -> Option<(&Hit, usize)> {
    hits.iter()
        .find_map(|hit| hit.input.filter(|_| hit.id == id).map(|index| (hit, index)))
}

impl View {
    /// The point in the text of the field of `hit` under `at`, a point in the scene as the consumer shows it.
    pub(super) fn local_point(
        &self,
        hit: &Hit,
        positions: &FieldGeometry,
        at: Point,
    ) -> Option<Point> {
        let at = self.moving.painted(hit.group, at);
        if positions.scale.x <= 0.0
            || positions.scale.y <= 0.0
            || !at.x.is_finite()
            || !at.y.is_finite()
        {
            return None;
        }
        Some(Point::new(
            (at.x - hit.rect.x) / positions.scale.x + positions.scroll.x,
            (at.y - hit.rect.y) / positions.scale.y + positions.scroll.y,
        ))
    }
}
