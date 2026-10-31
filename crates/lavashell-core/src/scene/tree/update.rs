// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Content, Namespace, NamespaceId, Node, NodeKey, SceneNodeId, SceneTree, Slot, StoredKind,
    StoredNode,
};
use crate::scene::Affine;
use crate::shape::RoundedRect;
use std::sync::Arc;

#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneUpdate {
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub changes: Vec<Change>,
}

/// One change to the nodes of a namespace. A `parent` of `None` means the top level of the namespace. `index` is a
/// position among the children of that parent.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Change {
    Insert {
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        node: Node,
    },
    Remove {
        id: SceneNodeId,
    },
    Move {
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        /// The position among the children of `parent` once the node left its old position.
        index: usize,
    },
    SetContent {
        id: SceneNodeId,
        content: Content,
    },
    SetSlot {
        id: SceneNodeId,
        slot: SlotChange,
    },
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SlotChange {
    Transform(Slot<Affine>),
    Opacity(Slot<f64>),
    Clip(Option<Slot<RoundedRect>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    #[error("the tree has no namespace {0}")]
    UnknownNamespace(NamespaceId),
    #[error("the namespace has no node {0}")]
    UnknownNode(SceneNodeId),
    #[error("the namespace already has a node {0}")]
    DuplicateNode(SceneNodeId),
    #[error("node {0} is not a group")]
    NotAGroup(SceneNodeId),
    #[error("node {0} is not a leaf")]
    NotALeaf(SceneNodeId),
    #[error("index {index} lies past the {children} children of the parent")]
    IndexPastEnd { index: usize, children: usize },
    #[error("node {0} would move inside itself")]
    Cycle(SceneNodeId),
    /// The grafted namespace is the root, or the grafting leaf sits in that namespace or in a namespace grafted below
    /// it.
    #[error("namespace {0} would paint inside itself")]
    GraftCycle(NamespaceId),
    #[error("namespace {0} is grafted at another leaf")]
    AlreadyGrafted(NamespaceId),
}

impl FromIterator<Change> for SceneUpdate {
    fn from_iter<I: IntoIterator<Item = Change>>(changes: I) -> Self {
        Self {
            changes: changes.into_iter().collect(),
        }
    }
}

impl SceneTree {
    /// Applies the changes of `update` to `namespace` in order and checks each change before it is made. An error
    /// stops the update. The changes before the error stay applied.
    ///
    /// # Errors
    ///
    /// Fails when the namespace is unknown, or with the first change that does not fit the tree.
    pub fn apply(&mut self, namespace: NamespaceId, update: SceneUpdate) -> Result<(), ApplyError> {
        if !self.namespaces.0.contains_key(&namespace) {
            return Err(ApplyError::UnknownNamespace(namespace));
        }
        update
            .changes
            .into_iter()
            .try_for_each(|change| self.change(namespace, change))
    }

    pub(super) fn change(
        &mut self,
        namespace: NamespaceId,
        change: Change,
    ) -> Result<(), ApplyError> {
        match change {
            Change::Insert {
                id,
                parent,
                index,
                node,
            } => self.insert(namespace, id, parent, index, node),
            Change::Remove { id } => self.remove(namespace, id),
            Change::Move { id, parent, index } => self.relocate(namespace, id, parent, index),
            Change::SetContent { id, content } => self.set_content(namespace, id, content),
            Change::SetSlot { id, slot } => self.set_slot(namespace, id, slot),
        }
    }

    fn insert(
        &mut self,
        namespace: NamespaceId,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
        node: Node,
    ) -> Result<(), ApplyError> {
        let key = NodeKey::new(namespace, id);
        let target = self.namespace(namespace)?;
        if target.node(id).is_some() {
            return Err(ApplyError::DuplicateNode(id));
        }
        check_index(index, target.children(parent)?.len())?;
        if let Node::Leaf(Content::Graft(grafted)) = &node {
            self.check_graft(key, *grafted)?;
            self.namespace_mut(*grafted)?.grafted_at = Some(key);
        }
        let kind = match node {
            Node::Group(group) => {
                self.track_motion(key, group.moves());
                StoredKind::Group {
                    group,
                    children: Arc::default(),
                }
            }
            Node::Leaf(content) => StoredKind::Leaf(content),
        };
        self.attach(key, parent, index, kind)
    }

    pub(super) fn attach(
        &mut self,
        key: NodeKey,
        parent: Option<SceneNodeId>,
        index: usize,
        kind: StoredKind,
    ) -> Result<(), ApplyError> {
        let placed = self.place();
        let target = self.namespace_mut(key.namespace)?;
        target.children_mut(parent)?.insert(index, key.node);
        target.insert_node(
            key.node,
            StoredNode {
                parent,
                placed,
                kind,
            },
        );
        Ok(())
    }

    fn remove(&mut self, namespace: NamespaceId, id: SceneNodeId) -> Result<(), ApplyError> {
        let target = self.namespace_mut(namespace)?;
        let parent = target.entry(id)?.parent;
        target.children_mut(parent)?.retain(|child| *child != id);
        let mut below = vec![id];
        while let Some(id) = below.pop() {
            let key = NodeKey::new(namespace, id);
            let _was_present_in_moving = self.moving.remove(&key);
            let entry = self.namespace_mut(namespace)?.remove_node(id);
            match entry.as_deref().map(|entry| &entry.kind) {
                Some(StoredKind::Group { children, .. }) => below.extend(children.iter()),
                Some(StoredKind::Leaf(Content::Graft(target))) => self.ungraft(key, *target),
                _ => {}
            }
        }
        Ok(())
    }

    fn relocate(
        &mut self,
        namespace: NamespaceId,
        id: SceneNodeId,
        parent: Option<SceneNodeId>,
        index: usize,
    ) -> Result<(), ApplyError> {
        let source = self.namespace(namespace)?;
        let old = source.entry(id)?.parent;
        let mut above = parent;
        while let Some(ancestor) = above {
            if ancestor == id {
                return Err(ApplyError::Cycle(id));
            }
            above = source.entry(ancestor)?.parent;
        }
        let children = source.children(parent)?.len() - usize::from(old == parent);
        check_index(index, children)?;
        let placed = self.place();
        let target = self.namespace_mut(namespace)?;
        target.children_mut(old)?.retain(|child| *child != id);
        target.children_mut(parent)?.insert(index, id);
        if let Some(entry) = target.node_mut(id) {
            entry.parent = parent;
            entry.placed = placed;
        }
        Ok(())
    }

    fn set_content(
        &mut self,
        namespace: NamespaceId,
        id: SceneNodeId,
        content: Content,
    ) -> Result<(), ApplyError> {
        let key = NodeKey::new(namespace, id);
        let StoredKind::Leaf(old) = &self.namespace(namespace)?.entry(id)?.kind else {
            return Err(ApplyError::NotALeaf(id));
        };
        let shown = match old {
            Content::Graft(shown) => Some(*shown),
            Content::Packet(_) => None,
        };
        if let Content::Graft(target) = content {
            self.check_graft(key, target)?;
        }
        if let Some(shown) = shown {
            self.ungraft(key, shown);
        }
        if let Content::Graft(target) = content {
            self.namespace_mut(target)?.grafted_at = Some(key);
        }
        if let Some(entry) = self.namespace_mut(namespace)?.node_mut(id) {
            entry.kind = StoredKind::Leaf(content);
        }
        Ok(())
    }

    fn set_slot(
        &mut self,
        namespace: NamespaceId,
        id: SceneNodeId,
        slot: SlotChange,
    ) -> Result<(), ApplyError> {
        let entry = self.namespace_mut(namespace)?.node_mut(id);
        let StoredKind::Group { group, .. } = &mut entry.ok_or(ApplyError::UnknownNode(id))?.kind
        else {
            return Err(ApplyError::NotAGroup(id));
        };
        match slot {
            SlotChange::Transform(transform) => group.transform = transform,
            SlotChange::Opacity(opacity) => group.opacity = opacity,
            SlotChange::Clip(clip) => group.clip = clip,
        }
        let moves = group.moves();
        self.track_motion(NodeKey::new(namespace, id), moves);
        Ok(())
    }

    fn track_motion(&mut self, key: NodeKey, moves: bool) {
        if moves {
            let _newly_inserted_in_moving = self.moving.insert(key);
        } else {
            let _was_present_in_moving = self.moving.remove(&key);
        }
    }

    fn place(&mut self) -> u64 {
        self.placements += 1;
        self.placements
    }

    fn check_graft(&self, leaf: NodeKey, target: NamespaceId) -> Result<(), ApplyError> {
        let grafted_at = self.namespace(target)?.grafted_at;
        if grafted_at.is_some_and(|at| at != leaf) {
            return Err(ApplyError::AlreadyGrafted(target));
        }
        if target == Self::ROOT {
            return Err(ApplyError::GraftCycle(target));
        }
        let mut current = Some(leaf.namespace);
        while let Some(namespace) = current {
            if namespace == target {
                return Err(ApplyError::GraftCycle(target));
            }
            current = self.namespace(namespace)?.grafted_at.map(|at| at.namespace);
        }
        Ok(())
    }

    fn ungraft(&mut self, leaf: NodeKey, target: NamespaceId) {
        let grafted = self.namespaces.0.get_mut(&target);
        if let Some(grafted) = grafted.filter(|grafted| grafted.grafted_at == Some(leaf)) {
            grafted.grafted_at = None;
        }
    }

    fn namespace(&self, id: NamespaceId) -> Result<&Namespace, ApplyError> {
        (self.namespaces.0.get(&id)).ok_or(ApplyError::UnknownNamespace(id))
    }

    fn namespace_mut(&mut self, id: NamespaceId) -> Result<&mut Namespace, ApplyError> {
        (self.namespaces.0.get_mut(&id)).ok_or(ApplyError::UnknownNamespace(id))
    }
}

fn check_index(index: usize, children: usize) -> Result<(), ApplyError> {
    if index > children {
        return Err(ApplyError::IndexPastEnd { index, children });
    }
    Ok(())
}

impl Namespace {
    fn entry(&self, id: SceneNodeId) -> Result<&StoredNode, ApplyError> {
        self.node(id).ok_or(ApplyError::UnknownNode(id))
    }

    pub(super) fn children(
        &self,
        parent: Option<SceneNodeId>,
    ) -> Result<&[SceneNodeId], ApplyError> {
        let Some(parent) = parent else {
            return Ok(self.top.as_slice());
        };
        match &self.entry(parent)?.kind {
            StoredKind::Group { children, .. } => Ok(children.as_slice()),
            StoredKind::Leaf(_) => Err(ApplyError::NotAGroup(parent)),
        }
    }

    fn children_mut(
        &mut self,
        parent: Option<SceneNodeId>,
    ) -> Result<&mut Vec<SceneNodeId>, ApplyError> {
        let Some(parent) = parent else {
            return Ok(Arc::make_mut(&mut self.top));
        };
        let entry = self
            .node_mut(parent)
            .ok_or(ApplyError::UnknownNode(parent))?;
        match &mut entry.kind {
            StoredKind::Group { children, .. } => Ok(Arc::make_mut(children)),
            StoredKind::Leaf(_) => Err(ApplyError::NotAGroup(parent)),
        }
    }
}
