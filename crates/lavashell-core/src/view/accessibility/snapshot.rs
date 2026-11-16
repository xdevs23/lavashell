// SPDX-License-Identifier: GPL-3.0-only
use crate::accessibility::{
    AccessibleNode, AccessibleState, AccessibleValue, ActionKind, RangeInfo, Role, Semantics,
    SemanticsMode, SemanticsState, TextSelection,
};
use crate::accessibility::{AccessibleText, ActionKinds};
use crate::geometry::Rect;
use crate::ui::{FieldContent, Lines, NodeId, NodeKind};
use crate::view::View;
use crate::view::component::index_of;
use crate::view::layout::Visibility;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default)]
pub(super) struct InheritedSemantics {
    disabled: bool,
    merged: bool,
}

#[derive(Clone, Copy)]
enum NamePart {
    Owner,
    Descendant,
}

impl View {
    pub(super) fn accessibility_subtree(
        &self,
        index: usize,
        inherited: InheritedSemantics,
        nodes: &mut BTreeMap<NodeId, AccessibleNode>,
        exported: &mut Vec<NodeId>,
    ) {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        if self.accessibility_hidden(index) {
            return;
        }
        let disabled = inherited.disabled || !node.enabled || semantics.state.disabled;
        let actions = self.accessibility_capabilities(index, disabled);
        let omitted = self.accessibility_omitted(index, inherited.merged, &actions);
        let mut children = Vec::new();
        if semantics.mode != SemanticsMode::ClearDescendants {
            let below = InheritedSemantics {
                disabled,
                merged: inherited.merged
                    || semantics.mode == SemanticsMode::MergeDescendants
                    || matches!(node.kind, NodeKind::Container(_)) && self.has_activation(index),
            };
            for child in &node.children {
                self.accessibility_subtree(*child, below, nodes, &mut children);
            }
        }
        if omitted {
            exported.extend(children);
            return;
        }
        let accessible = self.accessible_node(index, disabled, actions, children);
        let _no_node_with_the_same_id = nodes.insert(node.id, accessible);
        exported.push(node.id);
    }

    fn accessible_node(
        &self,
        index: usize,
        disabled: bool,
        actions: ActionKinds,
        children: Vec<NodeId>,
    ) -> AccessibleNode {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        let geometry = self
            .accessibility
            .geometry
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
            heading_level: semantics.heading_level,
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
        }
    }

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

    fn accessibility_text(
        &self,
        index: usize,
        state: &mut AccessibleState,
    ) -> Option<AccessibleText> {
        let node = &self.nodes[index];
        let semantics = self.accessibility_semantics(index);
        let selection = |value: &String| {
            self.editing.edits.get(&node.id).map(|edit| TextSelection {
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
        self.accessibility_name_parts(index, NamePart::Owner, &mut values);
        values.join(" ")
    }

    fn accessibility_relation_name(&self, index: usize) -> Option<String> {
        let names = self
            .accessibility_semantics(index)
            .relations
            .labelled_by
            .iter()
            .filter_map(|id| {
                let index = index_of(&self.nodes, *id)?;
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

    fn accessibility_name_parts(&self, index: usize, part: NamePart, values: &mut Vec<String>) {
        let node = &self.nodes[index];
        let activated_separately = match part {
            NamePart::Owner => false,
            NamePart::Descendant => self.has_activation(index),
        };
        if self.accessibility_hidden(index) || activated_separately {
            return;
        }
        if let NodeKind::Text(value, _) = &node.kind
            && !value.is_empty()
        {
            values.push(value.clone());
        }
        for child in &node.children {
            self.accessibility_name_parts(*child, NamePart::Descendant, values);
        }
    }

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
