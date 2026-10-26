// SPDX-License-Identifier: GPL-3.0-only
use super::{
    AccessibleNode, AccessibleState, AccessibleValue, ActionKind, BTreeMap, NodeId, NodeKind,
    RangeInfo, Rect, Role, Semantics, SemanticsMode, SemanticsState, TextSelection, View,
};
use crate::accessibility::{AccessibleText, ActionKinds};
use crate::ui::{FieldContent, Lines};
use crate::view::Visibility;

impl View {
    /// Adds the node at `index` and its descendants to `nodes`, except the ones left out of the accessibility tree.
    /// Appends to `exported` the ids that become children of the parent of the node: the id of the node, or the ids
    /// exported by its children where the tree leaves the node out.
    pub(super) fn accessibility_subtree(
        &self,
        index: usize,
        inherited_disabled: bool,
        suppress_passive: bool,
        nodes: &mut BTreeMap<NodeId, AccessibleNode>,
        exported: &mut Vec<NodeId>,
    ) {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        if self.accessibility_hidden(index) {
            return;
        }
        let disabled = inherited_disabled || !node.enabled || semantics.state.disabled;
        let actions = self.accessibility_capabilities(index, disabled);
        let omitted = self.accessibility_omitted(index, suppress_passive, &actions);
        let mut children = Vec::new();
        if semantics.mode != SemanticsMode::ClearDescendants {
            let merge = suppress_passive
                || semantics.mode == SemanticsMode::MergeDescendants
                || matches!(node.kind, NodeKind::Container(_)) && self.has_activation(index);
            for child in &node.children {
                self.accessibility_subtree(*child, disabled, merge, nodes, &mut children);
            }
        }
        if omitted {
            exported.extend(children);
            return;
        }
        let geometry = self
            .accessibility_geometry
            .get(&node.id)
            .copied()
            .unwrap_or_default();
        let mut state = AccessibleState {
            semantics: SemanticsState {
                disabled,
                modal: semantics.state.modal || node.modal,
                ..semantics.state.clone()
            },
            focusable: actions.contains(&ActionKind::Focus),
            selected: node.selected,
        };
        let name = self.accessibility_name(index);
        let protected = self.protected_index(index);
        let text = self.accessibility_text(index, &mut state);
        let value = if protected {
            None
        } else {
            semantics
                .value
                .clone()
                .or_else(|| {
                    text.as_ref()
                        .map(|text| AccessibleValue::Text(text.value.clone()))
                })
                .filter(|value| match value {
                    AccessibleValue::Number(number) => number.is_finite(),
                    AccessibleValue::Text(_) => true,
                })
        };
        let _no_node_with_the_same_id = nodes.insert(
            node.id,
            AccessibleNode {
                id: node.id,
                role: if protected {
                    Role::PasswordInput
                } else {
                    semantics
                        .role
                        .clone()
                        .unwrap_or_else(|| self.accessibility_role(index))
                },
                heading_level: semantics
                    .heading_level
                    .filter(|level| (1..=6).contains(level)),
                name,
                description: semantics
                    .description
                    .clone()
                    .or_else(|| node.tooltip.as_ref().map(|tooltip| tooltip.label.clone())),
                value,
                bounds: geometry.bounds,
                visible_bounds: (geometry.visible)
                    .and_then(|visible| visible.intersection(Rect::from_size(self.size))),
                children,
                state,
                actions,
                relations: semantics.relations.clone(),
                live: semantics.live,
                range: semantics.range.filter(RangeInfo::valid),
                collection: semantics.collection,
                item: semantics.item,
                text,
            },
        );
        exported.push(node.id);
    }

    /// Whether the accessibility tree leaves out the node at `index`: a decorative node, a node without interaction
    /// below a node that merges its descendants, or a spacer, icon or canvas that has default semantics, exports no
    /// state and has no action.
    fn accessibility_omitted(
        &self,
        index: usize,
        suppress_passive: bool,
        actions: &ActionKinds,
    ) -> bool {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        let interactive = self.has_activation(index)
            || matches!(node.kind, NodeKind::TextInput { .. })
            || node.scroll_axis().is_some()
            || actions
                .iter()
                .any(|kind| *kind != ActionKind::ScrollIntoView)
            || semantics
                .role
                .as_ref()
                .is_some_and(|role| !matches!(role, Role::Group | Role::Text | Role::Image));
        semantics.mode == SemanticsMode::Decorative
            || suppress_passive && !interactive
            || matches!(
                node.kind,
                NodeKind::Spacer(_) | NodeKind::Icon { .. } | NodeKind::Canvas { .. }
            ) && semantics == &Semantics::default()
                && !node.exports_state()
                && actions.is_empty()
    }

    fn accessibility_name(&self, index: usize) -> Option<String> {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        (semantics.name.clone())
            .or_else(|| self.accessibility_relation_name(index))
            .or_else(|| {
                if let NodeKind::Text(value, _) = &node.kind {
                    Some(value.clone())
                } else if self.has_activation(index)
                    || semantics.mode == SemanticsMode::MergeDescendants
                {
                    let value = self.accessibility_visible_name(index);
                    (!value.is_empty()).then_some(value)
                } else {
                    None
                }
            })
    }

    /// The accessible text of a text input or a text node, or the text set in the semantics of any other node. For
    /// selectable text, this function also marks `state` read-only.
    fn accessibility_text(
        &self,
        index: usize,
        state: &mut AccessibleState,
    ) -> Option<AccessibleText> {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        let selection = |value: &String| {
            self.edits.get(&node.id).map(|edit| TextSelection {
                anchor: scalar_offset(value, edit.anchor.unwrap_or(edit.cursor)),
                focus: scalar_offset(value, edit.cursor),
            })
        };
        if let NodeKind::TextInput {
            content,
            placeholder,
            ..
        } = &node.kind
        {
            let mut text = semantics.text.clone().unwrap_or_default();
            (text.value, text.selection, text.runs) = match content {
                FieldContent::Plain { value, .. } => (
                    value.clone(),
                    selection(value),
                    self.accessibility_text_runs(index),
                ),
                FieldContent::Protected(_) => (String::new(), None, Vec::new()),
            };
            text.placeholder = (!placeholder.is_empty()).then(|| placeholder.clone());
            text.multiline = content.lines() == Lines::Several;
            Some(text)
        } else if let NodeKind::Text(value, _) = &node.kind {
            let mut text = semantics.text.clone().unwrap_or_default();
            text.value.clone_from(value);
            if self.text_selectable(index) {
                state.semantics.form.read_only = true;
                text.selection = selection(value);
            }
            text.runs = self.accessibility_text_runs(index);
            Some(text)
        } else {
            semantics.text.clone()
        }
    }

    fn accessibility_role(&self, index: usize) -> Role {
        let node = &self.nodes[index];
        if index == 0 {
            Role::Window
        } else if node.modal {
            Role::Dialog
        } else if self.has_activation(index) {
            Role::Button
        } else if node.scroll_axis().is_some() {
            Role::ScrollView
        } else {
            match node.kind {
                NodeKind::Text(..) => Role::Text,
                NodeKind::TextInput { ref content, .. } => match content.lines() {
                    Lines::One => Role::TextInput,
                    Lines::Several => Role::TextArea,
                },
                NodeKind::Canvas { .. } | NodeKind::Icon { .. } => Role::Image,
                _ => Role::Group,
            }
        }
    }

    fn accessibility_visible_name(&self, index: usize) -> String {
        let mut values = Vec::new();
        self.accessibility_name_parts(index, true, &mut values);
        values.join(" ")
    }

    fn accessibility_relation_name(&self, index: usize) -> Option<String> {
        let names = self
            .accessibility_semantics(index)
            .relations
            .labelled_by
            .iter()
            .filter_map(|id| {
                let index = self.nodes.iter().position(|node| node.id == *id)?;
                let semantics = self.accessibility_semantics(index);
                if self.accessibility_hidden(index) {
                    return None;
                }
                semantics.name.clone().or_else(|| {
                    let name = self.accessibility_visible_name(index);
                    (!name.is_empty()).then_some(name)
                })
            })
            .collect::<Vec<_>>();
        (!names.is_empty()).then(|| names.join(" "))
    }

    fn accessibility_name_parts(&self, index: usize, owner: bool, values: &mut Vec<String>) {
        let node = &self.nodes[index];
        if self.accessibility_hidden(index) || !owner && self.has_activation(index) {
            return;
        }
        if let NodeKind::Text(value, _) = &node.kind
            && !value.is_empty()
        {
            values.push(value.clone());
        }
        for child in &node.children {
            self.accessibility_name_parts(*child, false, values);
        }
    }

    /// Whether the semantics of the node at `index` or a condition hides the node from assistive technology.
    fn accessibility_hidden(&self, index: usize) -> bool {
        self.accessibility_semantics(index).mode == SemanticsMode::Hidden
            || self.layout[index].visibility == Visibility::Hidden
    }

    pub(super) fn has_activation(&self, index: usize) -> bool {
        let node = &self.nodes[index];
        node.action.is_some() || node.activate.is_some()
    }

    pub(super) fn accessibility_capabilities(&self, index: usize, disabled: bool) -> ActionKinds {
        if disabled {
            return ActionKinds::default();
        }
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        let (activates, selectable) = (self.has_activation(index), self.text_selectable(index));
        let focuses = node.focusable
            && (node.scroll_axis().is_some()
                || activates
                || node.key_action.is_some()
                || selectable);
        let editable =
            matches!(node.kind, NodeKind::TextInput { .. }) && !semantics.state.form.read_only;
        let offered = [
            (focuses, ActionKind::Focus),
            (activates, ActionKind::Activate),
            (selectable, ActionKind::SetTextSelection),
            (editable, ActionKind::SetValue),
            (editable, ActionKind::ReplaceSelectedText),
            (
                self.scroll_ancestor(index).is_some(),
                ActionKind::ScrollIntoView,
            ),
        ];
        let mut actions: ActionKinds = (node.accessibility_actions.keys().copied())
            .chain(
                offered
                    .into_iter()
                    .filter_map(|(offers, kind)| offers.then_some(kind)),
            )
            .chain(node.scroll_axis().map(ActionKind::Scroll))
            .collect();
        if (semantics.range).is_some_and(|range| range.valid() && range.step.is_some())
            && actions.contains(&ActionKind::SetValue)
        {
            actions.extend([ActionKind::Increment, ActionKind::Decrement]);
        }
        if semantics.state.form.read_only {
            actions.retain(|kind| {
                !matches!(
                    kind,
                    ActionKind::SetValue
                        | ActionKind::ReplaceSelectedText
                        | ActionKind::Increment
                        | ActionKind::Decrement
                )
            });
        }
        if self.protected_index(index) {
            actions.retain(|kind| {
                !matches!(
                    kind,
                    ActionKind::SetValue
                        | ActionKind::ReplaceSelectedText
                        | ActionKind::SetTextSelection
                )
            });
        }
        actions
    }
}

pub(super) fn scalar_offset(value: &str, byte: usize) -> usize {
    value
        .char_indices()
        .take_while(|(index, _)| *index < byte)
        .count()
}
