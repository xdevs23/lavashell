// SPDX-License-Identifier: GPL-3.0-only
use crate::accessibility::{
    AccessibilityAction, AccessibleNode, AccessibleValue, ActionError, ActionOutcome,
    ActionRequest, CollectionInfo, IgnoredBecause,
};
use crate::geometry::{Axis, Point};
use crate::text_edit::TextEdit;
use crate::ui::{FieldContent, NodeKind};
use crate::units::Dp;
use crate::view::View;
use crate::view::component::index_of;
use crate::view::focus::FocusSource;
use crate::view::scroll::ScrollStyle;

impl View {
    pub fn accessibility_action(
        &mut self,
        request: ActionRequest,
    ) -> Result<ActionOutcome, ActionError> {
        self.refresh_stale_accessibility();
        let (index, action) = match self.resolved_action(request) {
            Ok(resolved) => resolved,
            Err(cause) => return Ok(ActionOutcome::Ignored(cause)),
        };
        let callback = (self.nodes[index].accessibility_actions)
            .get(&action.kind())
            .cloned();
        let handled = match callback {
            Some(callback) => Ok(callback.borrow_mut()(action)?),
            None => self.default_accessibility_action(index, action),
        };
        if let Err(cause) = handled {
            return Ok(ActionOutcome::Ignored(cause));
        }
        self.redraw = true;
        self.finish_input();
        self.refresh_accessibility();
        Ok(ActionOutcome::Performed)
    }

    fn resolved_action(
        &self,
        request: ActionRequest,
    ) -> Result<(usize, AccessibilityAction), IgnoredBecause> {
        let semantic = (self.accessibility.tree.nodes.get(&request.target))
            .ok_or(IgnoredBecause::UnknownTarget)?;
        if semantic.state.semantics.disabled {
            return Err(IgnoredBecause::Disabled);
        }
        if !semantic.actions.contains(&request.action.kind()) {
            return Err(IgnoredBecause::NotOffered);
        }
        if !request.action.is_finite() {
            return Err(IgnoredBecause::NotFinite);
        }
        let index = index_of(&self.nodes, request.target).ok_or(IgnoredBecause::StaleTarget)?;
        if let AccessibilityAction::RevealCollectionItem { row, column } = request.action {
            let collection = semantic.collection.ok_or(IgnoredBecause::NoCollection)?;
            if !reveals_in_bounds(collection, row, column) {
                return Err(IgnoredBecause::OutsideCollection);
            }
        }
        let action = match request.action {
            AccessibilityAction::SetValue(AccessibleValue::Number(value)) => {
                let value =
                    (semantic.range).map_or(value, |range| value.clamp(range.min, range.max));
                AccessibilityAction::SetValue(AccessibleValue::Number(value))
            }
            AccessibilityAction::Increment | AccessibilityAction::Decrement
                if !self.nodes[index]
                    .accessibility_actions
                    .contains_key(&request.action.kind()) =>
            {
                stepped_value(semantic, &request.action)?
            }
            action => action,
        };
        let text_into_range = self.accessibility_semantics(index).range.is_some()
            && matches!(
                action,
                AccessibilityAction::SetValue(AccessibleValue::Text(_))
            );
        if text_into_range {
            return Err(IgnoredBecause::TextIntoRange);
        }
        Ok((index, action))
    }

    fn default_accessibility_action(
        &mut self,
        index: usize,
        action: AccessibilityAction,
    ) -> Result<(), IgnoredBecause> {
        let id = self.nodes[index].id;
        match action {
            AccessibilityAction::Focus => {
                self.accessibility_reveal(index);
                self.move_focus_with_source(id, FocusSource::Assistive);
                Ok(())
            }
            AccessibilityAction::Activate => {
                let gestures = &self.nodes[index].gestures;
                let (activate, click) = (gestures.activate().cloned(), gestures.click().cloned());
                self.run_activation(activate, click, Point::default());
                Ok(())
            }
            AccessibilityAction::Scroll(_, delta) => {
                self.scroll_node_by(index, delta, ScrollStyle::Animated);
                Ok(())
            }
            AccessibilityAction::ScrollIntoView => {
                self.accessibility_reveal(index);
                Ok(())
            }
            AccessibilityAction::SetValue(AccessibleValue::Text(value)) => {
                let value = self.normalize_for_field(index, &value);
                self.accessibility_edit(index, |edit, current| {
                    edit.select_all(current);
                    edit.replace(current, &value);
                })
            }
            AccessibilityAction::ReplaceSelectedText(value) => {
                let value = self.normalize_for_field(index, &value);
                self.accessibility_edit(index, |edit, current| edit.replace(current, &value))
            }
            AccessibilityAction::SetTextSelection(selection) => {
                let (NodeKind::Text(value, ..)
                | NodeKind::TextInput {
                    content: FieldContent::Plain { value, .. },
                    ..
                }) = &self.nodes[index].kind
                else {
                    return Err(IgnoredBecause::NotText);
                };
                let anchor = byte_offset(value, selection.anchor)
                    .ok_or(IgnoredBecause::SelectionOutsideText)?;
                let focus = byte_offset(value, selection.focus)
                    .ok_or(IgnoredBecause::SelectionOutsideText)?;
                if let Some(shaped) =
                    (self.editing.positions.get(&id)).filter(|shaped| shaped.value == *value)
                    && !shaped.shaped_range(value, &(anchor..focus))
                {
                    return Err(IgnoredBecause::SelectionSplitsCluster);
                }
                let edit = self
                    .editing
                    .edits
                    .entry(id)
                    .or_insert_with(|| TextEdit::at_end(value));
                edit.cursor = focus;
                edit.anchor = Some(anchor);
                edit.line = None;
                edit.goal_x = None;
                Ok(())
            }
            _ => Err(IgnoredBecause::Unsupported),
        }
    }

    fn accessibility_edit(
        &mut self,
        index: usize,
        update: impl FnOnce(&mut TextEdit, &mut String),
    ) -> Result<(), IgnoredBecause> {
        let id = self.nodes[index].id;
        let NodeKind::TextInput {
            content: FieldContent::Plain { value, change, .. },
            ..
        } = &mut self.nodes[index].kind
        else {
            return Err(IgnoredBecause::NotEditable);
        };
        let before = value.clone();
        let edit = self
            .editing
            .edits
            .entry(id)
            .or_insert_with(|| TextEdit::at_end(value));
        update(edit, value);
        if *value != before {
            let value = value.clone();
            let change = change.clone();
            change.borrow_mut()(value);
        }
        Ok(())
    }

    pub(super) fn scroll_ancestor(&self, index: usize) -> Option<(usize, Axis)> {
        (self.parents.ancestry(index).skip(1))
            .find_map(|parent| Some((parent, self.nodes[parent].scroll_axis()?)))
    }

    fn accessibility_reveal(&mut self, index: usize) {
        let mut target = self.layout[index].rect;
        let mut current = index;
        while let Some((parent, axis)) = self.scroll_ancestor(current) {
            let geometry = self
                .accessibility
                .geometry
                .get(&self.nodes[parent].id)
                .copied()
                .unwrap_or_default();
            let viewport = geometry.content;
            let delta = if axis.start(target) < axis.start(viewport) {
                axis.start(target) - axis.start(viewport)
            } else if axis.end(target) > axis.end(viewport) {
                axis.end(target) - axis.end(viewport)
            } else {
                Dp::ZERO
            };
            if delta != Dp::ZERO {
                self.scroll_node_by_window(parent, delta);
                target = target.translated(axis.vector(-delta));
            }
            current = parent;
        }
    }
}

fn reveals_in_bounds(collection: CollectionInfo, row: usize, column: usize) -> bool {
    if collection.rows.is_some_and(|rows| row >= rows)
        || collection.columns.is_some_and(|columns| column >= columns)
    {
        return false;
    }
    let Some(count) = collection.item_count else {
        return true;
    };
    (collection.columns)
        .and_then(|columns| row.checked_mul(columns)?.checked_add(column))
        .is_some_and(|index| index < count)
}

fn stepped_value(
    semantic: &AccessibleNode,
    action: &AccessibilityAction,
) -> Result<AccessibilityAction, IgnoredBecause> {
    let range = semantic.range.ok_or(IgnoredBecause::NoRange)?;
    let step = range.step.ok_or(IgnoredBecause::NoStep)?;
    let Some(AccessibleValue::Number(value)) = semantic.value else {
        return Err(IgnoredBecause::NoNumber);
    };
    let change = if *action == AccessibilityAction::Increment {
        step
    } else {
        -step
    };
    Ok(AccessibilityAction::SetValue(AccessibleValue::Number(
        (value + change).clamp(range.min, range.max),
    )))
}

fn byte_offset(value: &str, scalar: usize) -> Option<usize> {
    if scalar == value.chars().count() {
        Some(value.len())
    } else {
        value.char_indices().nth(scalar).map(|(byte, _)| byte)
    }
}
