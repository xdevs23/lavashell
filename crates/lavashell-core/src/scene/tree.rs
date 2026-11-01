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
mod snapshot;
#[cfg(test)]
mod test_changes;
#[cfg(test)]
mod tests;
mod trie;
#[cfg(test)]
mod trie_tests;
mod update;

pub use damage::DamageTracker;
pub use order::{PaintOrder, PaintVisitor, PlacedLeaf, Presented};
pub(crate) use presence::PresenceVectors;
pub(crate) use presence::Stretch;
pub use presence::{Presence, PresenceMap, PresenceParts, ScaleOrigin};
pub use slot::{Clamp, Group, GroupValues, Slot, SlotMotion, SlotValue, TransformMotion};
pub use snapshot::SceneSnapshot;
pub use update::{ApplyError, Change, SceneUpdate, SlotChange};

use crate::geometry::Rect;
use crate::retained::Footprint;
use crate::scene::DrawCommand;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use trie::Trie;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceId(u64);

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

#[derive(Clone)]
enum StoredKind {
    Group {
        group: Group,
        children: Arc<Vec<SceneNodeId>>,
    },
    Leaf(Content),
}

#[derive(Clone)]
struct StoredNode {
    parent: Option<SceneNodeId>,
    /// The number of the insert or move that last put the node at its place in paint order. The tree counts its
    /// inserts and moves from 1.
    placed: u64,
    kind: StoredKind,
}

#[derive(Clone, Default)]
struct Namespace {
    nodes: Trie<StoredNode>,
    top: Arc<Vec<SceneNodeId>>,
    grafted_at: Option<NodeKey>,
}

/// The nodes in every namespace of a tree. Snapshots taken from the tree share them until the tree changes them.
#[derive(Clone)]
struct Namespaces(BTreeMap<NamespaceId, Namespace>);

pub struct SceneTree {
    id: TreeId,
    namespaces: Namespaces,
    next_namespace: u64,
    placements: u64,
    moving: BTreeSet<NodeKey>,
}

impl Default for SceneTree {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: TreeId(NEXT.fetch_add(1, Ordering::Relaxed)),
            namespaces: Namespaces(BTreeMap::from([(Self::ROOT, Namespace::default())])),
            next_namespace: Self::ROOT.0 + 1,
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        (self.namespaces.0.values()).all(|namespace| namespace.nodes.is_empty())
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

    pub(crate) fn children(
        &self,
        namespace: NamespaceId,
        parent: Option<SceneNodeId>,
    ) -> &[SceneNodeId] {
        (self.namespaces.0.get(&namespace))
            .and_then(|namespace| namespace.children(parent).ok())
            .unwrap_or_default()
    }

    pub(crate) fn group(&self, key: NodeKey) -> Option<&Group> {
        self.namespaces.group(key)
    }

    pub(crate) fn packet(&self, key: NodeKey) -> Option<&Packet> {
        self.namespaces.packet(key)
    }

    pub fn create_namespace(&mut self) -> NamespaceId {
        let id = NamespaceId(self.next_namespace);
        self.next_namespace += 1;
        let _replaced_in_namespaces = self.namespaces.0.insert(id, Namespace::default());
        id
    }
}

impl Namespace {
    fn node(&self, id: SceneNodeId) -> Option<&StoredNode> {
        self.nodes.get(id.0)
    }

    fn node_mut(&mut self, id: SceneNodeId) -> Option<&mut StoredNode> {
        self.nodes.get_mut(id.0)
    }

    fn insert_node(&mut self, id: SceneNodeId, node: StoredNode) {
        self.nodes.insert(id.0, node);
    }

    fn remove_node(&mut self, id: SceneNodeId) -> Option<Arc<StoredNode>> {
        self.nodes.remove(id.0)
    }
}

impl Namespaces {
    fn node(&self, key: NodeKey) -> Option<&StoredNode> {
        self.0.get(&key.namespace)?.node(key.node)
    }

    fn placed(&self, key: NodeKey) -> Option<u64> {
        self.node(key).map(|node| node.placed)
    }

    fn group(&self, key: NodeKey) -> Option<&Group> {
        match &self.node(key)?.kind {
            StoredKind::Group { group, .. } => Some(group),
            StoredKind::Leaf(_) => None,
        }
    }

    fn packet(&self, key: NodeKey) -> Option<&Packet> {
        match &self.node(key)?.kind {
            StoredKind::Leaf(Content::Packet(packet)) => Some(packet),
            StoredKind::Leaf(Content::Graft(_)) | StoredKind::Group { .. } => None,
        }
    }
}
