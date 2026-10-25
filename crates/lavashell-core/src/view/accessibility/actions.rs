// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibilityAction, AccessibleValue, ActionError, ActionOutcome, ActionRequest, Axis, Dp,
    FocusSource, NodeKind, Point, TextEdit, View,
};
use crate::ui::FieldContent;

impl View {
    /// Performs an action requested by an assistive client, such as focusing, activating or scrolling the target
    /// node. Call it on the UI thread. The view returns [`ActionOutcome::Ignored`] and calls no handler of the
    /// application when the target was removed, lies outside the open modal, is disabled or does not list the action,
    /// and when the request has a malformed argument, such as a number that is not finite.
    ///
    /// # Errors
    ///
    /// Fails with the error returned by the handler that the application registered on the target for the action.
    pub fn accessibility_action(
        &mut self,
        request: ActionRequest,
    ) -> Result<ActionOutcome, ActionError> {
        if self.needs_accessibility_update() {
            self.refresh_accessibility();
        }
        let Some(semantic) = self.accessibility.nodes.get(&request.target) else {
            return Ok(ActionOutcome::Ignored);
        };
        if semantic.state.semantics.disabled
            || !semantic.actions.contains(&request.action.kind())
            || !request.action.is_finite()
        {
            return Ok(ActionOutcome::Ignored);
        }
        let Some(index) = self.nodes.iter().position(|node| node.id == request.target) else {
            return Ok(ActionOutcome::Ignored);
        };
        if let AccessibilityAction::RevealCollectionItem { row, column } = request.action {
            let Some(collection) = semantic.collection else {
                return Ok(ActionOutcome::Ignored);
            };
            if collection.rows.is_some_and(|rows| row >= rows)
                || collection.columns.is_some_and(|columns| column >= columns)
            {
                return Ok(ActionOutcome::Ignored);
            }
            if let Some(count) = collection.item_count {
                let Some(columns) = collection.columns else {
                    return Ok(ActionOutcome::Ignored);
                };
                let Some(index) = row
                    .checked_mul(columns)
                    .and_then(|index| index.checked_add(column))
                else {
                    return Ok(ActionOutcome::Ignored);
                };
                if index >= count {
                    return Ok(ActionOutcome::Ignored);
                }
            }
        }
        let action = match request.action {
            AccessibilityAction::SetValue(AccessibleValue::Number(value)) => {
                if let Some(range) = semantic.range {
                    AccessibilityAction::SetValue(AccessibleValue::Number(
                        value.clamp(range.min, range.max),
                    ))
                } else {
                    AccessibilityAction::SetValue(AccessibleValue::Number(value))
                }
            }
            AccessibilityAction::Increment | AccessibilityAction::Decrement
                if !self.nodes[index]
                    .accessibility_actions
                    .contains_key(&request.action.kind()) =>
            {
                let Some(range) = semantic.range else {
                    return Ok(ActionOutcome::Ignored);
                };
                let Some(step) = range.step else {
                    return Ok(ActionOutcome::Ignored);
                };
                let Some(AccessibleValue::Number(value)) = semantic.value else {
                    return Ok(ActionOutcome::Ignored);
                };
                let change = if request.action == AccessibilityAction::Increment {
                    step
                } else {
                    -step
                };
                AccessibilityAction::SetValue(AccessibleValue::Number(
                    (value + change).clamp(range.min, range.max),
                ))
            }
            action => action,
        };
        if self.accessibility_semantics(index).range.is_some()
            && matches!(
                action,
                AccessibilityAction::SetValue(AccessibleValue::Text(_))
            )
        {
            return Ok(ActionOutcome::Ignored);
        }
        let handled = if let Some(callback) = self.nodes[index]
            .accessibility_actions
            .get(&action.kind())
            .cloned()
        {
            callback.borrow_mut()(action)?;
            true
        } else {
            self.default_accessibility_action(index, action)
        };
        if !handled {
            return Ok(ActionOutcome::Ignored);
        }
        self.redraw = true;
        self.finish_input();
        self.refresh_accessibility();
        Ok(ActionOutcome::Performed)
    }

    fn default_accessibility_action(&mut self, index: usize, action: AccessibilityAction) -> bool {
        let id = self.nodes[index].id;
        match action {
            AccessibilityAction::Focus => {
                self.accessibility_reveal(index);
                self.focus = Some(id);
                self.record_focus_source(FocusSource::Assistive);
                true
            }
            AccessibilityAction::Activate => {
                let node = &self.nodes[index];
                let (activate, action) = (node.activate.clone(), node.action.clone());
                self.run_activation(activate, action, Point::default());
                true
            }
            AccessibilityAction::Scroll(_, delta) => {
                self.scroll_node_by(index, delta, false);
                true
            }
            AccessibilityAction::ScrollIntoView => {
                // The accessibility tree lists this action only for a node inside a scrolling node. A request for an
                // action that the target does not list was ignored above.
                self.accessibility_reveal(index);
                true
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
                let (NodeKind::Text(value, _)
                | NodeKind::TextInput {
                    content: FieldContent::Plain { value, .. },
                    ..
                }) = &self.nodes[index].kind
                else {
                    return false;
                };
                let Some(anchor) = byte_offset(value, selection.anchor) else {
                    return false;
                };
                let Some(focus) = byte_offset(value, selection.focus) else {
                    return false;
                };
                if let Some(shaped) =
                    (self.text_positions.get(&id)).filter(|shaped| shaped.value == *value)
                    && !shaped.shaped_range(value, &(anchor..focus))
                {
                    return false;
                }
                let edit = self
                    .edits
                    .entry(id)
                    .or_insert_with(|| TextEdit::at_end(value));
                edit.cursor = focus;
                edit.anchor = Some(anchor);
                edit.line = None;
                edit.goal_x = None;
                true
            }
            _ => false,
        }
    }

    fn accessibility_edit(
        &mut self,
        index: usize,
        update: impl FnOnce(&mut TextEdit, &mut String),
    ) -> bool {
        let id = self.nodes[index].id;
        let NodeKind::TextInput {
            content: FieldContent::Plain { value, change, .. },
            ..
        } = &mut self.nodes[index].kind
        else {
            return false;
        };
        let before = value.clone();
        let edit = self
            .edits
            .entry(id)
            .or_insert_with(|| TextEdit::at_end(value));
        update(edit, value);
        if *value != before {
            let value = value.clone();
            let change = change.clone();
            change.borrow_mut()(value);
        }
        true
    }

    /// The index of the nearest scrolling ancestor of the node at `index`, with the axis along which that ancestor
    /// scrolls.
    pub(super) fn scroll_ancestor(&self, index: usize) -> Option<(usize, Axis)> {
        (self.parents.ancestry(index).skip(1))
            .find_map(|parent| Some((parent, self.nodes[parent].scroll_axis()?)))
    }

    fn accessibility_reveal(&mut self, index: usize) {
        let mut target = self.layout[index].rect;
        let mut current = index;
        while let Some((parent, axis)) = self.scroll_ancestor(current) {
            let geometry = self
                .accessibility_geometry
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

fn byte_offset(value: &str, scalar: usize) -> Option<usize> {
    if scalar == value.chars().count() {
        Some(value.len())
    } else {
        value.char_indices().nth(scalar).map(|(byte, _)| byte)
    }
}
