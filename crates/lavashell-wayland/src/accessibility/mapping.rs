// SPDX-License-Identifier: GPL-3.0-only
use super::{ids::Ids, properties, text};
use accesskit::{Node, Rect, Role as NativeRole, TreeId, TreeInfo, TreeUpdate};
use lavashell_core::accessibility::{AccessibilityTree, AccessibleNode, ActionKind, Role};
use lavashell_core::prelude::Axis;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn tree(tree: &AccessibilityTree, ids: &mut Ids, title: &str) -> TreeUpdate {
    let root = ids.node(tree.root);
    let mut nodes = Vec::new();
    let parents: BTreeMap<_, _> = tree
        .nodes
        .values()
        .flat_map(|node| node.children.iter().map(move |child| (*child, node.id)))
        .collect();
    for source in tree.nodes.values() {
        let id = ids.node(source.id);
        let mut node = Node::new(role(&source.role));
        node.set_children(
            source
                .children
                .iter()
                .map(|child| ids.node(*child))
                .collect::<Vec<_>>(),
        );
        if source.id == tree.root && source.name.is_none() {
            node.set_label(title);
        }
        properties::apply(source, &mut node, ids);
        if let Some(index) = collection_index(tree, source, &parents) {
            node.set_position_in_set(index);
        }
        text::apply(source, &mut node, ids, &mut nodes);
        nodes.push((id, node));
    }
    ids.retain(&nodes.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>());
    let mut info = TreeInfo::new(root);
    info.toolkit_name = Some("Lavashell".into());
    info.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    TreeUpdate {
        nodes,
        tree: Some(info),
        tree_id: TreeId::ROOT,
        focus: tree
            .focus
            .filter(|id| tree.nodes.contains_key(id))
            .map_or(root, |id| ids.node(id)),
    }
}

fn collection_index(
    tree: &AccessibilityTree,
    source: &AccessibleNode,
    parents: &BTreeMap<lavashell_core::prelude::NodeId, lavashell_core::prelude::NodeId>,
) -> Option<usize> {
    let item = source.item?;
    let mut parent = parents.get(&source.id);
    while let Some(id) = parent {
        let node = tree.nodes.get(id)?;
        if let Some(collection) = node.collection {
            return item
                .row
                .checked_mul(collection.columns.unwrap_or(1))?
                .checked_add(item.column);
        }
        parent = parents.get(id);
    }
    Some(item.row)
}

pub(super) fn difference(full: &TreeUpdate, previous: Option<&TreeUpdate>) -> TreeUpdate {
    let Some(previous) = previous.filter(|previous| previous.tree == full.tree) else {
        return full.clone();
    };
    let old: BTreeMap<_, _> = previous
        .nodes
        .iter()
        .map(|(id, node)| (*id, node))
        .collect();
    TreeUpdate {
        nodes: full
            .nodes
            .iter()
            .filter(|(id, node)| old.get(id) != Some(&node))
            .cloned()
            .collect(),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: full.focus,
    }
}

pub(super) fn rect(rect: lavashell_core::prelude::Rect) -> Rect {
    Rect::new(
        rect.x.get(),
        rect.y.get(),
        (rect.x + rect.width).get(),
        (rect.y + rect.height).get(),
    )
}

fn role(role: &Role) -> NativeRole {
    match role {
        Role::Window => NativeRole::Window,
        Role::Text => NativeRole::Label,
        Role::Button => NativeRole::Button,
        Role::Checkbox => NativeRole::CheckBox,
        Role::RadioButton => NativeRole::RadioButton,
        Role::Switch => NativeRole::Switch,
        Role::Slider => NativeRole::Slider,
        Role::ProgressIndicator => NativeRole::ProgressIndicator,
        Role::TextInput => NativeRole::TextInput,
        Role::PasswordInput => NativeRole::PasswordInput,
        Role::TextArea => NativeRole::MultilineTextInput,
        Role::Image => NativeRole::Image,
        Role::Link => NativeRole::Link,
        Role::List => NativeRole::List,
        Role::ListItem => NativeRole::ListItem,
        Role::Menu => NativeRole::Menu,
        Role::MenuItem => NativeRole::MenuItem,
        Role::Dialog => NativeRole::Dialog,
        Role::Heading => NativeRole::Heading,
        Role::Tab => NativeRole::Tab,
        Role::TabList => NativeRole::TabList,
        Role::ScrollView => NativeRole::ScrollView,
        Role::ScrollBar => NativeRole::ScrollBar,
        Role::Group | Role::Custom(_) => NativeRole::Group,
    }
}

pub(super) fn actions(source: &AccessibleNode, node: &mut Node) {
    use accesskit::Action as Native;
    for action in &source.actions {
        let native = match action {
            ActionKind::Focus => Native::Focus,
            ActionKind::Activate => Native::Click,
            ActionKind::SetValue => Native::SetValue,
            ActionKind::Increment => Native::Increment,
            ActionKind::Decrement => Native::Decrement,
            ActionKind::ScrollIntoView => Native::ScrollIntoView,
            ActionKind::SetTextSelection => Native::SetTextSelection,
            ActionKind::ReplaceSelectedText => Native::ReplaceSelectedText,
            ActionKind::Scroll(Axis::Horizontal) => {
                node.add_action(Native::ScrollLeft);
                Native::ScrollRight
            }
            ActionKind::Scroll(Axis::Vertical) => {
                node.add_action(Native::ScrollUp);
                Native::ScrollDown
            }
            ActionKind::RevealCollectionItem => continue,
        };
        node.add_action(native);
    }
}
