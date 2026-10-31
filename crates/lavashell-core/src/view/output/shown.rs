// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupId, Painted};
use crate::scene::tree::{
    Change, Content, Group, Node, NodeKey, Packet, SceneNodeId, SceneTree, Slot, SlotChange,
};
use crate::scene::{Affine, ChangeSink, SinkId};
use crate::view::FrameError;
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

#[derive(Default)]
pub(in crate::view) struct Shown {
    /// The sink that received the last update, or none before the first update and after a failed restart.
    sink: Option<SinkId>,
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
    pub fn is_shown_to(&self, sink: SinkId) -> bool {
        self.sink == Some(sink)
    }

    /// Sets the sink of the next update. When `sink` differs from the last one, the next update builds the whole
    /// scene.
    pub fn retarget(&mut self, sink: Option<SinkId>) {
        if self.sink != sink {
            self.sink = sink;
            self.forget_scene();
        }
    }

    fn forget_scene(&mut self) {
        self.ids.clear();
        self.identities.clear();
        self.items.clear();
        self.tree = SceneTree::default();
    }

    /// Sends to `sink` each change that makes the scene in `sink` match `items`, then ends the frame.
    /// `followed` replaces the painted transforms of the groups listed in it. After an error, the next update builds
    /// the whole scene.
    pub fn update<S: ChangeSink>(
        &mut self,
        items: &[Painted],
        followed: &BTreeMap<GroupId, Slot<Affine>>,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        (self.send_changes(items, followed, sink))
            .and_then(|()| sink.end_frame().map_err(FrameError::Sink))
            .inspect_err(|_| self.forget_scene())
    }

    fn send_changes<S: ChangeSink>(
        &mut self,
        items: &[Painted],
        followed: &BTreeMap<GroupId, Slot<Affine>>,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        self.place_children(None, None, items, &mut Counters::default(), sink)?;
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
                self.change(Change::SetSlot { id, slot }, sink)?;
            }
        }
        Ok(())
    }

    fn change<S: ChangeSink>(
        &mut self,
        change: Change,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        (self.tree.show_change(change.clone())).map_err(FrameError::Mirror)?;
        sink.change(change).map_err(FrameError::Sink)
    }

    fn place_children<S: ChangeSink>(
        &mut self,
        parent: Option<SceneNodeId>,
        enclosing: Option<GroupId>,
        items: &[Painted],
        counters: &mut Counters,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        for (index, item) in items.iter().enumerate() {
            self.place(parent, index, enclosing, item, counters, sink)?;
        }
        let children = self.tree.children(SceneTree::ROOT, parent);
        let leftovers = children.get(items.len()..).unwrap_or_default().to_vec();
        for id in leftovers {
            self.forget(id);
            self.change(Change::Remove { id }, sink)?;
        }
        Ok(())
    }

    fn place<S: ChangeSink>(
        &mut self,
        parent: Option<SceneNodeId>,
        index: usize,
        enclosing: Option<GroupId>,
        item: &Painted,
        counters: &mut Counters,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
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
                    return self.insert(id, parent, index, node, sink);
                };
                let same =
                    Arc::ptr_eq(shown.commands(), commands) || shown.commands()[..] == commands[..];
                if !same {
                    let content = Content::Packet(Packet::new(commands.clone()));
                    self.change(Change::SetContent { id, content }, sink)?;
                }
                self.position(id, parent, index, sink)
            }
            Painted::Scope { values, items } => {
                let identity = NodeIdentity::Scope {
                    enclosing,
                    index: counters.scopes,
                };
                counters.scopes += 1;
                let id = self.id(identity);
                self.group(id, parent, index, &(*values).into(), sink)?;
                self.place_children(Some(id), enclosing, items, counters, sink)
            }
            Painted::Group {
                id: group_id,
                group,
                items,
            } => {
                let id = self.id(NodeIdentity::Group(*group_id));
                self.group(id, parent, index, group, sink)?;
                let shown = self.items.get(&id);
                if shown.is_some_and(|shown| Arc::ptr_eq(shown, items)) {
                    return Ok(());
                }
                let _items_shown_before = self.items.insert(id, items.clone());
                let mut counters = Counters::default();
                self.place_children(Some(id), Some(*group_id), items, &mut counters, sink)
            }
        }
    }

    fn group<S: ChangeSink>(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        group: &Group,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        let Some(shown) = self.tree.group(mirrored_key(id)) else {
            return self.insert(id, parent, index, Node::Group(group.clone()), sink);
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
            self.change(Change::SetSlot { id, slot }, sink)?;
        }
        self.position(id, parent, index, sink)
    }

    fn insert<S: ChangeSink>(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        node: Node,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        let insert = Change::Insert {
            id,
            parent,
            index,
            node,
        };
        self.change(insert, sink)
    }

    fn position<S: ChangeSink>(
        &mut self,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        sink: &mut S,
    ) -> Result<(), FrameError<S::Error>> {
        if self.tree.children(SceneTree::ROOT, parent).get(index) == Some(&id) {
            return Ok(());
        }
        self.change(Change::Move { id, parent, index }, sink)
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
