// SPDX-License-Identifier: GPL-3.0-only
//! A scene tree of groups and leaves, split into namespaces. Each producer writes the nodes of one namespace and
//! changes them through updates. A leaf can graft one namespace into another.

mod commands;
mod damage;
#[cfg(test)]
mod damage_tests;
#[cfg(all(test, feature = "serde"))]
mod encoding_tests;
mod order;
mod presence;
mod slot;
#[cfg(test)]
mod slot_tests;
#[cfg(test)]
mod test_changes;
#[cfg(test)]
mod tests;
mod update;

pub use damage::DamageTracker;
pub use order::{PaintOrder, PaintVisitor, PlacedLeaf, Presented};
pub(crate) use presence::PresenceVectors;
pub(crate) use presence::Stretch;
pub use presence::{Presence, PresenceMap, PresenceParts, ScaleOrigin};
pub use slot::{Clamp, Group, GroupValues, Slot, SlotMotion, SlotValue, TransformMotion};
pub use update::{ApplyError, Change, SceneUpdate, SlotChange};

use crate::geometry::Rect;
use crate::retained::Footprint;
use crate::scene::DrawCommand;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// Tells apart every namespace created in the process. The root namespace of each tree is `SceneTree::ROOT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceId(u64);

/// The number of the next namespace created in the process. Every number below it names a root or a created namespace.
static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(SceneTree::ROOT.0 + 1);

impl NamespaceId {
    fn next() -> Self {
        Self(NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed))
    }

    /// Whether some tree in the process created this namespace, even if it removed it since.
    fn was_created(self) -> bool {
        self.0 < NEXT_NAMESPACE.load(Ordering::Relaxed)
    }
}

/// A namespace made by `SceneTree::create_namespace`. A tree can remove it, unlike its root namespace, and removing it
/// consumes this value.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CreatedNamespace(NamespaceId);

impl CreatedNamespace {
    #[must_use]
    pub fn id(&self) -> NamespaceId {
        self.0
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneNodeId(pub u64);

impl std::fmt::Display for SceneNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TreeId(u64);

impl TreeId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketGeneration(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeKey {
    pub namespace: NamespaceId,
    pub node: SceneNodeId,
}

impl NodeKey {
    #[must_use]
    pub const fn new(namespace: NamespaceId, node: SceneNodeId) -> Self {
        Self { namespace, node }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "PacketCommands")
)]
pub struct Packet {
    #[cfg_attr(feature = "serde", serde(skip))]
    generation: PacketGeneration,
    #[cfg_attr(
        feature = "serde",
        serde(serialize_with = "crate::encoding::shared_slice::serialize")
    )]
    commands: Arc<[DrawCommand]>,
    #[cfg_attr(feature = "serde", serde(skip))]
    footprint: OnceLock<Footprint>,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(rename = "Packet")]
struct PacketCommands {
    #[serde(deserialize_with = "crate::encoding::shared_slice::deserialize")]
    commands: Arc<[DrawCommand]>,
}

#[cfg(feature = "serde")]
impl From<PacketCommands> for Packet {
    fn from(packet: PacketCommands) -> Self {
        Self::new(packet.commands)
    }
}

impl Packet {
    /// Wraps `commands` in a packet with a new generation, larger than the generation of every packet made before
    /// in the process.
    pub fn new(commands: Arc<[DrawCommand]>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            generation: PacketGeneration(NEXT.fetch_add(1, Ordering::Relaxed)),
            commands,
            footprint: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn generation(&self) -> PacketGeneration {
        self.generation
    }

    #[must_use]
    pub fn commands(&self) -> &Arc<[DrawCommand]> {
        &self.commands
    }

    /// Analyzed when it is first read, so that a read packet allocates nothing for it while it is read.
    #[must_use]
    pub fn footprint(&self) -> &Footprint {
        (self.footprint).get_or_init(|| Footprint::analyze(&self.commands))
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Content {
    Packet(Packet),
    Graft(
        #[cfg_attr(feature = "serde", serde(with = "crate::encoding::process_local"))] NamespaceId,
    ),
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Node {
    Group(Group),
    Leaf(Content),
}

enum StoredKind {
    Group {
        group: Group,
        children: Vec<SceneNodeId>,
    },
    Leaf(Content),
}

struct StoredNode {
    parent: Option<SceneNodeId>,
    /// The number of the insert or move that last put the node at its place in paint order. The tree counts its
    /// inserts and moves from 1.
    placed: u64,
    kind: StoredKind,
}

#[derive(Default)]
struct Namespace {
    nodes: BTreeMap<SceneNodeId, StoredNode>,
    top: Vec<SceneNodeId>,
    grafted_at: Option<NodeKey>,
}

pub struct SceneTree {
    id: TreeId,
    namespaces: BTreeMap<NamespaceId, Namespace>,
    placements: u64,
    moving: BTreeSet<NodeKey>,
}

impl Default for SceneTree {
    fn default() -> Self {
        Self {
            id: TreeId::next(),
            namespaces: BTreeMap::from([(Self::ROOT, Namespace::default())]),
            placements: 0,
            moving: BTreeSet::new(),
        }
    }
}

impl SceneTree {
    pub const ROOT: NamespaceId = NamespaceId(0);

    #[must_use]
    pub fn id(&self) -> TreeId {
        self.id
    }

    /// Whether the root namespace has no nodes, as after `reset_root`, whatever the created namespaces contain.
    #[must_use]
    pub fn is_root_empty(&self) -> bool {
        (self.namespaces.get(&Self::ROOT)).is_none_or(|root| root.nodes.is_empty())
    }

    /// The leaf of a tree made by [`SceneTree::with_leaf`].
    pub const LEAF: NodeKey = NodeKey::new(Self::ROOT, SceneNodeId(1));

    /// A tree whose only node is the leaf [`SceneTree::LEAF`], which shows `commands` in a new packet.
    ///
    /// # Errors
    ///
    /// Fails when the leaf cannot be attached to the empty tree.
    pub fn with_leaf(commands: impl Into<Arc<[DrawCommand]>>) -> Result<Self, ApplyError> {
        let mut tree = Self::default();
        let packet = Packet::new(commands.into());
        tree.attach(
            Self::LEAF,
            None,
            0,
            StoredKind::Leaf(Content::Packet(packet)),
        )?;
        Ok(tree)
    }

    /// # Errors
    ///
    /// Fails with the first change of `update` that does not fit the tree.
    pub fn show(&mut self, update: SceneUpdate) -> Result<(), ApplyError> {
        self.apply(Self::ROOT, update)
    }

    pub(crate) fn show_change(&mut self, change: Change) -> Result<(), ApplyError> {
        self.change(Self::ROOT, change)
    }

    fn node(&self, key: NodeKey) -> Option<&StoredNode> {
        self.namespaces.get(&key.namespace)?.nodes.get(&key.node)
    }

    fn placed(&self, key: NodeKey) -> Option<u64> {
        self.node(key).map(|node| node.placed)
    }

    pub(crate) fn children(
        &self,
        namespace: NamespaceId,
        parent: Option<SceneNodeId>,
    ) -> &[SceneNodeId] {
        (self.namespaces.get(&namespace))
            .and_then(|namespace| namespace.children(parent).ok())
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn group(&self, key: NodeKey) -> Option<&Group> {
        match &self.node(key)?.kind {
            StoredKind::Group { group, .. } => Some(group),
            StoredKind::Leaf(_) => None,
        }
    }

    pub(crate) fn packet(&self, key: NodeKey) -> Option<&Packet> {
        match &self.node(key)?.kind {
            StoredKind::Leaf(Content::Packet(packet)) => Some(packet),
            StoredKind::Leaf(Content::Graft(_)) | StoredKind::Group { .. } => None,
        }
    }

    pub(crate) fn content(&self, key: NodeKey) -> Option<&Content> {
        match &self.node(key)?.kind {
            StoredKind::Leaf(content) => Some(content),
            StoredKind::Group { .. } => None,
        }
    }

    /// A new empty namespace, whose id no other namespace in the process has.
    pub fn create_namespace(&mut self) -> CreatedNamespace {
        let id = NamespaceId::next();
        let _replaced_in_namespaces = self.namespaces.insert(id, Namespace::default());
        CreatedNamespace(id)
    }

    /// Removes `namespace` and its nodes. A leaf that grafts it shows nothing from then on, also a leaf inserted
    /// later, and the id is never given to another namespace. Removing a namespace that does not exist changes nothing.
    pub fn remove_namespace(&mut self, CreatedNamespace(namespace): CreatedNamespace) {
        if let Some(removed) = self.namespaces.remove(&namespace) {
            self.drop_nodes(namespace, removed);
        }
    }

    /// Empties the root namespace and gives the tree a new id, so that the next frame of a view shows its whole scene
    /// in it. The created namespaces remain, and a leaf of that frame grafts each of them again.
    pub fn reset_root(&mut self) {
        let emptied = self.namespaces.get_mut(&Self::ROOT).map(std::mem::take);
        if let Some(emptied) = emptied {
            self.drop_nodes(Self::ROOT, emptied);
        }
        self.id = TreeId::next();
    }

    fn drop_nodes(&mut self, namespace: NamespaceId, dropped: Namespace) {
        self.moving.retain(|key| key.namespace != namespace);
        for (node, stored) in dropped.nodes {
            if let StoredKind::Leaf(Content::Graft(target)) = stored.kind {
                self.ungraft(NodeKey::new(namespace, node), target);
            }
        }
    }
}
