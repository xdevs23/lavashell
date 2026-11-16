// SPDX-License-Identifier: GPL-3.0-only
use super::TextDropTarget;
use crate::geometry::Point;
use crate::ui::{NodeId, NodeKind};
use crate::view::View;
use crate::view::component::index_of;
use std::ops::Range;

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
    #[must_use]
    pub fn text_source_contains(&self, source: &TextSelectionSource, at: Point) -> bool {
        self.routing.hits.iter().any(|hit| {
            hit.id == source.id && hit.input.is_some() && hit.contains(at, &self.routing.moving)
        })
    }

    #[must_use]
    pub fn text_source_drag_ready(&self, source: &TextSelectionSource, at: Point) -> bool {
        self.routing
            .hits
            .iter()
            .find(|hit| hit.id == source.id && hit.input.is_some())
            .zip(self.editing.positions.get(&source.id))
            .and_then(|(hit, positions)| self.routing.local_point(hit, positions, at))
            .is_some_and(|point| {
                self.editing
                    .edits
                    .get(&source.id)
                    .is_some_and(|edit| edit.drag_ready(point, self.touch_slop()))
            })
    }

    #[must_use]
    pub fn text_selection_source(&self, id: NodeId) -> Option<TextSelectionSource> {
        let index = index_of(&self.nodes, id)?;
        if self.protected_index(index)
            || !self.current_text_target(index)
            || !self.text_selectable(index)
        {
            return None;
        }
        let value = self.text_value(index)?;
        let range = self.editing.edits.get(&id)?.selection();
        if range.is_empty()
            || !self
                .editing
                .positions
                .get(&id)?
                .shaped_range(&value, &range)
        {
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

    #[must_use]
    pub fn text_selection_source_at(&self, at: Point) -> Option<TextSelectionSource> {
        let hit = self.routing.hit_at(at).filter(|hit| hit.input.is_some())?;
        let source = self.text_selection_source(hit.id)?;
        let positions = self.editing.positions.get(&hit.id)?;
        let point = self.routing.local_point(hit, positions, at)?;
        positions
            .geometry
            .selection(source.range.clone())
            .iter()
            .any(|rect| rect.contains(point))
            .then_some(source)
    }

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

    pub fn move_text_at(&mut self, source: &TextSelectionSource, target: &TextDropTarget) -> bool {
        if target.id != source.id || target.value != source.value {
            return false;
        }
        let Some(index) = self.text_move_index(source) else {
            return false;
        };
        let insertion = target.offset..target.offset;
        if !(self.editing.positions.get(&source.id))
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
        let index = self.validated_text_target(source.id, &source.value, &source.range)?;
        source.movable.then_some(index)
    }
}
