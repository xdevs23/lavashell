// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupId, Painted};
use crate::scene::Affine;
use crate::scene::tree::{
    ApplyError, Change, Content, Group, Node, NodeKey, Packet, SceneNodeId, SceneTree, SceneUpdate,
    Slot, SlotChange, TreeId,
};
use std::collections::BTreeMap;
use std::sync::Arc;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NodeIdentity {
    Group(GroupId),
    Leaf {
        enclosing: Option<GroupId>,
        index: usize,
    },
    Scope {
        enclosing: Option<GroupId>,
        index: usize,
    },
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::view) enum Recipient {
    #[default]
    Caller,
    Tree(TreeId),
}

#[derive(Default)]
pub(in crate::view) struct Shown {
    recipient: Recipient,
    tree: SceneTree,
    ids: BTreeMap<NodeIdentity, SceneNodeId>,
    identities: BTreeMap<SceneNodeId, NodeIdentity>,
    /// The items last placed under each group. When a group gets the same list again, the view skips placing the
    /// children of that group.
    items: BTreeMap<SceneNodeId, Arc<[Painted]>>,
    next_id: u64,
}

#[derive(Default)]
struct Counters {
    leaves: usize,
    scopes: usize,
}

impl Shown {
    pub fn is_shown_to(&self, recipient: Recipient) -> bool {
        self.recipient == recipient
    }

    /// Sets the recipient of the next update. When `recipient` differs from the last one, the next update builds the
    /// whole scene.
    pub fn retarget(&mut self, recipient: Recipient) {
        if !self.is_shown_to(recipient) {
            self.recipient = recipient;
            self.forget_scene();
        }
    }

    fn forget_scene(&mut self) {
        self.ids.clear();
        self.identities.clear();
        self.items.clear();
        self.tree = SceneTree::default();
    }

    /// The changes that make the scene of the recipient match `items`. `followed` replaces the painted transforms of
    /// the groups listed in it. After an error, the next update builds the whole scene.
    pub fn update(
        &mut self,
        items: &[Painted],
        followed: &BTreeMap<GroupId, Slot<Affine>>,
    ) -> Result<SceneUpdate, ApplyError> {
        let mut changes = Vec::new();
        (self.collect_changes(items, followed, &mut changes))
            .inspect_err(|_| self.forget_scene())?;
        Ok(SceneUpdate { changes })
    }

    fn collect_changes(
        &mut self,
        items: &[Painted],
        followed: &BTreeMap<GroupId, Slot<Affine>>,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        self.place_children(None, None, items, &mut Counters::default(), changes)?;
        for (group, transform) in followed {
            let Some(&id) = self.ids.get(&NodeIdentity::Group(*group)) else {
                continue;
            };
            if self
                .tree
                .group(mirrored_key(id))
                .is_some_and(|shown| shown.transform != *transform)
            {
                let slot = SlotChange::Transform(transform.clone());
                self.change(Change::SetSlot { id, slot }, changes)?;
            }
        }
        Ok(())
    }

    fn change(&mut self, change: Change, changes: &mut Vec<Change>) -> Result<(), ApplyError> {
        self.tree.show_change(change.clone())?;
        changes.push(change);
        Ok(())
    }

    fn place_children(
        &mut self,
        parent: Option<SceneNodeId>,
        enclosing: Option<GroupId>,
        items: &[Painted],
        counters: &mut Counters,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        for (index, item) in items.iter().enumerate() {
            self.place(parent, index, enclosing, item, counters, changes)?;
        }
        let children = self.tree.children(SceneTree::ROOT, parent);
        let leftovers = children.get(items.len()..).unwrap_or_default().to_vec();
        for id in leftovers {
            self.forget(id);
            self.change(Change::Remove { id }, changes)?;
        }
        Ok(())
    }

    fn place(
        &mut self,
        parent: Option<SceneNodeId>,
        index: usize,
        enclosing: Option<GroupId>,
        item: &Painted,
        counters: &mut Counters,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        match item {
            Painted::Leaf { commands } => {
                let identity = NodeIdentity::Leaf {
                    enclosing,
                    index: counters.leaves,
                };
                counters.leaves += 1;
                let id = self.id(identity);
                let Some(shown) = self.tree.packet(mirrored_key(id)) else {
                    let node = Node::Leaf(Content::Packet(Packet::new(commands.clone())));
                    return self.insert(id, parent, index, node, changes);
                };
                let same =
                    Arc::ptr_eq(shown.commands(), commands) || shown.commands()[..] == commands[..];
                if !same {
                    let content = Content::Packet(Packet::new(commands.clone()));
                    self.change(Change::SetContent { id, content }, changes)?;
                }
                self.position(id, parent, index, changes)
            }
            Painted::Scope { values, items } => {
                let identity = NodeIdentity::Scope {
                    enclosing,
                    index: counters.scopes,
                };
                counters.scopes += 1;
                let id = self.id(identity);
                self.group(id, parent, index, &(*values).into(), changes)?;
                self.place_children(Some(id), enclosing, items, counters, changes)
            }
            Painted::Group {
                id: group_id,
                group,
                items,
            } => {
                let id = self.id(NodeIdentity::Group(*group_id));
                self.group(id, parent, index, group, changes)?;
                let shown = self.items.get(&id);
                if shown.is_some_and(|shown| Arc::ptr_eq(shown, items)) {
                    return Ok(());
                }
                let _items_shown_before = self.items.insert(id, items.clone());
                let mut counters = Counters::default();
                self.place_children(Some(id), Some(*group_id), items, &mut counters, changes)
            }
        }
    }

    fn group(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        group: &Group,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        let Some(shown) = self.tree.group(mirrored_key(id)) else {
            return self.insert(id, parent, index, Node::Group(group.clone()), changes);
        };
        let mut slots = Vec::new();
        if shown.transform != group.transform {
            slots.push(SlotChange::Transform(group.transform.clone()));
        }
        if shown.opacity != group.opacity {
            slots.push(SlotChange::Opacity(group.opacity.clone()));
        }
        if shown.clip != group.clip {
            slots.push(SlotChange::Clip(group.clip.clone()));
        }
        for slot in slots {
            self.change(Change::SetSlot { id, slot }, changes)?;
        }
        self.position(id, parent, index, changes)
    }

    fn insert(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        node: Node,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        let insert = Change::Insert {
            id,
            parent,
            index,
            node,
        };
        self.change(insert, changes)
    }

    fn position(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        changes: &mut Vec<Change>,
    ) -> Result<(), ApplyError> {
        if self.tree.children(SceneTree::ROOT, parent).get(index) == Some(&id) {
            return Ok(());
        }
        self.change(Change::Move { id, parent, index }, changes)
    }

    fn id(&mut self, identity: NodeIdentity) -> SceneNodeId {
        if let Some(id) = self.ids.get(&identity) {
            return *id;
        }
        self.next_id += 1;
        let id = SceneNodeId(self.next_id);
        let _none_for_a_new_identity = self.ids.insert(identity, id);
        let _none_for_a_new_id = self.identities.insert(id, identity);
        id
    }

    /// Removes `id` and the nodes below it from the maps of identities and items. Call it before the removal of `id`
    /// from the tree, because it reads the nodes below `id` from the tree.
    fn forget(&mut self, id: SceneNodeId) {
        let mut below = vec![id];
        while let Some(id) = below.pop() {
            if let Some(identity) = self.identities.remove(&id) {
                let _id_of_a_forgotten_node = self.ids.remove(&identity);
            }
            let _items_of_a_forgotten_node = self.items.remove(&id);
            if self.tree.group(mirrored_key(id)).is_some() {
                below.extend_from_slice(self.tree.children(SceneTree::ROOT, Some(id)));
            }
        }
    }
}

fn mirrored_key(id: SceneNodeId) -> NodeKey {
    NodeKey::new(SceneTree::ROOT, id)
}
