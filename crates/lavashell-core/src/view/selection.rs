// SPDX-License-Identifier: GPL-3.0-only
use super::component::index_of;
use super::field::TextEditing;
use super::input::InputRouting;
use super::{Hit, View};
use crate::geometry::Point;
use crate::reactive::Phase;
use crate::text_edit::{FieldGeometry, TextEdit};
use crate::ui::{FieldContent, Lines, NodeId, NodeKind};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::ops::Range;

mod source;

pub use source::TextSelectionSource;

mod paste;

pub use paste::TextPasteTarget;

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
    #[must_use]
    pub fn editing_target(&self) -> Option<NodeId> {
        (self.session_field())
            .filter(|index| !self.protected_index(*index))
            .map(|index| self.nodes[index].id)
    }

    pub fn copy_selection(&mut self) -> Option<String> {
        let id = self.routing.selection_target()?;
        let index = index_of(&self.nodes, id)?;
        if self.protected_index(index) {
            return None;
        }
        let value = self.text_value(index)?.into_owned();
        let editing = self.editing.edits.get_mut(&id)?;
        editing.clamp(&value);
        let selection = editing.selection();
        if selection.is_empty() {
            return None;
        }
        Some(value[selection].to_string())
    }

    #[must_use]
    pub fn text_drop_target(&self, at: Point) -> Option<TextDropTarget> {
        let hit = self.routing.hit_at(at)?;
        let index = hit.input?.index;
        if !self.writable_text_target(index) {
            return None;
        }
        let value = self.text_value(index)?;
        let positions = self
            .editing
            .positions
            .get(&hit.id)
            .filter(|p| p.value == *value)?;
        let point = self.routing.local_point(hit, positions, at)?;
        Some(TextDropTarget {
            id: hit.id,
            value: value.to_string(),
            offset: positions.geometry.hit(point),
        })
    }

    pub fn insert_text_at(&mut self, target: &TextDropTarget, text: &str) -> bool {
        let insertion = target.offset..target.offset;
        let Some(index) = self.validated_text_target(target.id, &target.value, &insertion) else {
            return false;
        };
        let text = self.normalize_for_field(index, text);
        self.edit_at(index, |edit, value| {
            edit.move_to(target.offset, false);
            edit.replace(value, &text);
        });
        true
    }

    fn validated_text_target(
        &mut self,
        id: NodeId,
        value: &str,
        range: &Range<usize>,
    ) -> Option<usize> {
        self.refresh_stale_accessibility();
        let index = index_of(&self.nodes, id)?;
        let current = self.writable_text_target(index)
            && self.text_value(index).as_deref() == Some(value)
            && (self.editing.positions.get(&id)).is_some_and(|p| p.shaped_range(value, range));
        current.then_some(index)
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

    fn writable_text_target(&self, index: usize) -> bool {
        !self.protected_index(index)
            && self.current_text_target(index)
            && matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
            && !self.accessibility_semantics(index).state.form.read_only
    }

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
            .editing
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

    pub(super) fn press_text_at(&mut self, id: NodeId, at: Point) {
        let shift = self.routing.modifiers.shift;
        self.change_selection_at(id, at, |edit, positions, point| {
            edit.press(&positions.value, &positions.geometry, point, shift);
        });
    }

    pub(super) fn drag_text_to(&mut self, id: NodeId, at: Point) {
        let slop = self.slop();
        self.change_selection_at(id, at, |edit, positions, point| {
            edit.drag_to(&positions.geometry, point, slop);
        });
    }

    fn change_selection_at(
        &mut self,
        id: NodeId,
        at: Point,
        change: impl FnOnce(&mut TextEdit, &FieldGeometry, Point),
    ) {
        let Some((hit, index)) = input_hit(&self.routing.hits, id) else {
            return;
        };
        let Some(positions) = self.editing.positions.get(&hit.id) else {
            return;
        };
        if self.text_value(index).as_deref() != Some(positions.value.as_str()) {
            return;
        }
        let Some(point) = self.routing.local_point(hit, positions, at) else {
            return;
        };
        change(
            self.editing.edits.entry(hit.id).or_default(),
            positions,
            point,
        );
        self.redraw = true;
    }

    pub(super) fn release_text(&mut self, id: NodeId, at: Point) {
        let Some((hit, index)) = input_hit(&self.routing.hits, id) else {
            if let Some(edit) = self.editing.edits.get_mut(&id) {
                edit.cancel_drag();
            }
            return;
        };
        if !self.current_text_target(index) {
            if let Some(edit) = self.editing.edits.get_mut(&hit.id) {
                edit.cancel_drag();
            }
            return;
        }
        let inside = hit.contains(at, &self.routing.moving);
        let allow_move = matches!(self.nodes[index].kind, NodeKind::TextInput { .. })
            && !self.accessibility_semantics(index).state.form.read_only;
        self.drag_text_to(id, at);
        self.edit_at(index, |edit, value| edit.release(value, allow_move, inside));
    }

    pub(super) fn cancel_text_drag(&mut self) {
        let cancelled: BTreeSet<_> = (self.editing.edits.iter_mut())
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

fn input_hit(hits: &[Hit], id: NodeId) -> Option<(&Hit, usize)> {
    hits.iter().find_map(|hit| {
        hit.input
            .filter(|_| hit.id == id)
            .map(|target| (hit, target.index))
    })
}

impl View {}

impl TextEditing {
    pub(super) fn text_drag_in_progress(&self) -> bool {
        self.edits.values().any(TextEdit::is_dragging)
    }
}

impl InputRouting {
    pub(super) fn selection_target(&self) -> Option<NodeId> {
        self.focus.node().filter(|id| {
            self.hits
                .iter()
                .any(|hit| hit.id == *id && hit.input.is_some())
        })
    }
    pub(super) fn local_point(
        &self,
        hit: &Hit,
        positions: &FieldGeometry,
        at: Point,
    ) -> Option<Point> {
        let origin = hit.input?.origin;
        let at = self.moving.painted(hit.area.group, at);
        if positions.scale.x <= 0.0
            || positions.scale.y <= 0.0
            || !at.x.is_finite()
            || !at.y.is_finite()
        {
            return None;
        }
        Some(Point::new(
            (at.x - origin.x) / positions.scale.x + positions.scroll.x,
            (at.y - origin.y) / positions.scale.y + positions.scroll.y,
        ))
    }
}
