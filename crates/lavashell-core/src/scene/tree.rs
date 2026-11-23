// SPDX-License-Identifier: GPL-3.0-only

mod change;
mod commands;
mod damage;
#[cfg(test)]
mod damage_tests;
mod effect;
#[cfg(test)]
mod effect_tests;
#[cfg(all(test, feature = "serde"))]
mod encoding_tests;
mod order;
mod presence;
mod slot;
#[cfg(test)]
mod slot_tests;
mod snapshot;
#[cfg(test)]
pub(super) mod test_changes;
#[cfg(test)]
mod tests;
mod trie;
#[cfg(test)]
mod trie_tests;

pub use change::{ApplyError, Change, SlotChange};
pub use damage::DamageTracker;
pub use effect::{ChangeEffect, ChangeField, ChangeKey};
pub use order::{PaintOrder, PaintVisitor, PlacedLeaf, Presented};
pub(crate) use presence::PresenceVectors;
pub(crate) use presence::Stretch;
pub use presence::{Presence, PresenceMap, PresenceParts, ScaleOrigin};
pub use slot::{Clamp, Group, GroupValues, Slot, SlotMotion, SlotValue, TransformMotion};
pub use snapshot::SceneSnapshot;

use crate::geometry::Rect;
use crate::retained::{Footprint, UnbalancedRestore};
use crate::scene::DrawCommand;
use crate::unique_id::UniqueId;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
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
pub struct TreeId(UniqueId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketGeneration(UniqueId);

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
    serde(try_from = "PacketCommands")
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
    footprint: Footprint,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(rename = "Packet")]
struct PacketCommands {
    #[serde(deserialize_with = "crate::encoding::shared_slice::deserialize")]
    commands: Arc<[DrawCommand]>,
}

#[cfg(feature = "serde")]
impl TryFrom<PacketCommands> for Packet {
    type Error = UnbalancedRestore;

    fn try_from(packet: PacketCommands) -> Result<Self, UnbalancedRestore> {
        Self::new(packet.commands)
    }
}

impl Packet {
    pub fn new(commands: Arc<[DrawCommand]>) -> Result<Self, UnbalancedRestore> {
        Ok(Self {
            generation: PacketGeneration(UniqueId::next()),
            footprint: Footprint::analyze(&commands)?,
            commands,
        })
    }

    #[must_use]
    pub fn generation(&self) -> PacketGeneration {
        self.generation
    }

    #[must_use]
    pub fn commands(&self) -> &Arc<[DrawCommand]> {
        &self.commands
    }

    #[must_use]
    pub fn footprint(&self) -> &Footprint {
        &self.footprint
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
    placed: u64,
    kind: StoredKind,
}

#[derive(Clone, Default)]
struct Namespace {
    nodes: Trie<StoredNode>,
    top: Arc<Vec<SceneNodeId>>,
    grafted_at: Option<NodeKey>,
}

#[derive(Clone)]
struct Namespaces(BTreeMap<NamespaceId, Namespace>);

pub struct SceneTree {
    id: TreeId,
    namespaces: Namespaces,
    next_namespace: u64,
    placements: u64,
    moving: Arc<BTreeSet<NodeKey>>,
}

impl Default for SceneTree {
    fn default() -> Self {
        Self {
            id: TreeId(UniqueId::next()),
            namespaces: Namespaces(BTreeMap::from([(Self::ROOT, Namespace::default())])),
            next_namespace: Self::ROOT.0 + 1,
            placements: 0,
            moving: Arc::default(),
        }
    }
}

impl SceneTree {
    pub const ROOT: NamespaceId = NamespaceId(0);

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.namespaces.len()
    }

    pub const LEAF: NodeKey = NodeKey::new(Self::ROOT, SceneNodeId(1));

    pub fn with_leaf(commands: impl Into<Arc<[DrawCommand]>>) -> Result<Self, ApplyError> {
        let mut tree = Self::default();
        let packet = Packet::new(commands.into())?;
        tree.attach(
            Self::LEAF,
            None,
            0,
            StoredKind::Leaf(Content::Packet(packet)),
        )?;
        Ok(tree)
    }

    pub(crate) fn children(
        &self,
        namespace: NamespaceId,
        parent: Option<SceneNodeId>,
    ) -> Result<&[SceneNodeId], ApplyError> {
        self.namespace(namespace)?.children(parent)
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
    fn is_empty(&self) -> bool {
        (self.0.values()).all(|namespace| namespace.nodes.is_empty())
    }

    fn len(&self) -> usize {
        (self.0.values())
            .map(|namespace| namespace.nodes.len())
            .sum()
    }

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

#[cfg(any(test, feature = "test-support"))]
pub fn show_all(
    tree: &mut SceneTree,
    changes: impl IntoIterator<Item = Change>,
) -> Result<(), ApplyError> {
    (changes.into_iter()).try_for_each(|change| tree.apply_change(SceneTree::ROOT, change))
}
