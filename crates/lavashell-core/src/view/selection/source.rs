// SPDX-License-Identifier: GPL-3.0-only
use super::{NodeId, NodeKind, Point, TextDropTarget, View};
use std::ops::Range;

/// The selected text of a field, recorded before the host copies or drags it: the field, the text shown by the field,
/// and the byte range of the selection in that text.
#[derive(Clone)]
pub struct TextSelectionSource {
    id: NodeId,
    value: String,
    range: Range<usize>,
    movable: bool,
}

impl TextSelectionSource {
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.value[self.range.clone()]
    }

    #[must_use]
    pub fn can_move(&self) -> bool {
        self.movable
    }
}

impl View {
    /// Whether `at` lies inside the field from which `source` was recorded. The view moves selected text within that
    /// field by itself.
    #[must_use]
    pub fn text_source_contains(&self, source: &TextSelectionSource, at: Point) -> bool {
        self.hits
            .iter()
            .any(|hit| hit.id == source.id && hit.input.is_some() && hit.contains(at, &self.moving))
    }

    #[must_use]
    pub fn text_source_drag_ready(&self, source: &TextSelectionSource, at: Point) -> bool {
        self.hits
            .iter()
            .find(|hit| hit.id == source.id && hit.input.is_some())
            .zip(self.text_positions.get(&source.id))
            .and_then(|(hit, positions)| self.local_point(hit, positions, at))
            .is_some_and(|point| {
                self.edits
                    .get(&source.id)
                    .is_some_and(|edit| edit.drag_ready(point, self.touch_slop()))
            })
    }

    /// Records the selection of the node `id` without changing the node, whatever node has the focus. Returns `None`
    /// for a protected field, for a node without selectable text, and for an empty selection.
    #[must_use]
    pub fn text_selection_source(&self, id: NodeId) -> Option<TextSelectionSource> {
        let index = self.nodes.iter().position(|node| node.id == id)?;
        if self.protected_index(index)
            || !self.current_text_target(index)
            || !self.text_selectable(index)
        {
            return None;
        }
        let value = self.text_value(index)?;
        let range = self.edits.get(&id)?.selection();
        if range.is_empty() || !self.text_positions.get(&id)?.shaped_range(&value, &range) {
            return None;
        }
        Some(TextSelectionSource {
            id,
            value: value.to_string(),
            range,
            movable: matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
                && !self.accessibility_semantics(index).state.form.read_only,
        })
    }

    /// Records the selection of the field under `at` as a drag source, but only where `at` lies over the selected
    /// glyphs.
    #[must_use]
    pub fn text_selection_source_at(&self, at: Point) -> Option<TextSelectionSource> {
        let hit = self.hit_at(at).filter(|hit| hit.input.is_some())?;
        let source = self.text_selection_source(hit.id)?;
        let positions = self.text_positions.get(&hit.id)?;
        let point = self.local_point(hit, positions, at)?;
        positions
            .geometry
            .selection(source.range.clone())
            .iter()
            .any(|rect| rect.contains(point))
            .then_some(source)
    }

    /// Removes the selected text of `source` from its field. A host calls this only after the transfer reports a
    /// successful move. Returns false and changes nothing when the field no longer shows the recorded text or can no
    /// longer be edited.
    pub fn complete_text_move(&mut self, source: &TextSelectionSource) -> bool {
        let Some(index) = self.text_move_index(source) else {
            return false;
        };
        self.edit_at(index, |edit, value| {
            edit.cancel_drag();
            edit.cursor = source.range.end;
            edit.anchor = Some(source.range.start);
            edit.replace(value, "");
        });
        true
    }

    /// Moves the selected text of `source` to `target` in one edit, when `target` lies in the same field and that
    /// field still shows the recorded text. The caller must have checked that the received text came from `source`.
    pub fn move_text_at(&mut self, source: &TextSelectionSource, target: &TextDropTarget) -> bool {
        if target.id != source.id || target.value != source.value {
            return false;
        }
        let Some(index) = self.text_move_index(source) else {
            return false;
        };
        let insertion = target.offset..target.offset;
        if !(self.text_positions.get(&source.id))
            .is_some_and(|positions| positions.shaped_range(&source.value, &insertion))
        {
            return false;
        }
        self.edit_at(index, |edit, value| {
            edit.cancel_drag();
            edit.move_range(value, source.range.clone(), target.offset);
        });
        true
    }

    fn text_move_index(&mut self, source: &TextSelectionSource) -> Option<usize> {
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
        let index = self.nodes.iter().position(|node| node.id == source.id)?;
        if !source.movable
            || !self.writable_text_target(index)
            || self.text_value(index).as_deref() != Some(source.value.as_str())
            || !(self.text_positions.get(&source.id))
                .is_some_and(|positions| positions.shaped_range(&source.value, &source.range))
        {
            return None;
        }
        Some(index)
    }
}
