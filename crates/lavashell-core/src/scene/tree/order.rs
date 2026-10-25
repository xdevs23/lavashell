// SPDX-License-Identifier: GPL-3.0-only
use super::{
    BTreeMap, Content, DrawCommand, GroupValues, Namespace, NamespaceId, NodeKey, Packet,
    SceneNodeId, SceneTree, StoredKind, StoredNode,
};
use crate::scene::Affine;
use std::time::Duration;

/// Visits the groups, grafts and leaves of a scene tree in paint order. The `world` transform passed for a group or
/// a leaf maps the coordinates inside the group, or of the leaf, into the coordinates of the root namespace.
pub trait PaintVisitor<'a> {
    /// The value returned by `group` or `graft`, passed back to `exit` once the walk leaves that group or graft.
    type Saved;

    fn group(&mut self, key: NodeKey, values: GroupValues, world: Affine) -> Self::Saved;

    fn graft(&mut self, key: NodeKey, namespace: NamespaceId) -> Self::Saved;

    fn leaf(&mut self, leaf: PlacedLeaf<'a>);

    fn exit(&mut self, saved: Self::Saved);
}

/// Every `Group` and `Graft` is followed by the steps for its children or for the grafted namespace, then by one
/// `Exit`.
enum Step<'a> {
    Group {
        key: NodeKey,
        values: GroupValues,
        world: Affine,
    },
    Graft {
        key: NodeKey,
        namespace: NamespaceId,
    },
    Leaf(PlacedLeaf<'a>),
    Exit,
}

pub struct PlacedLeaf<'a> {
    pub key: NodeKey,
    pub packet: &'a Packet,
    pub world: Affine,
}

pub struct PaintOrder<'a> {
    namespaces: &'a BTreeMap<NamespaceId, Namespace>,
    now: Duration,
    levels: Vec<Level<'a>>,
}

struct Level<'a> {
    namespace: NamespaceId,
    nodes: &'a BTreeMap<SceneNodeId, StoredNode>,
    children: std::slice::Iter<'a, SceneNodeId>,
    world: Affine,
}

/// A scene tree whose animated slots are sampled at `now`. `now` is the presentation time, measured from the same
/// epoch as the time passed to `View::frame_at`.
#[derive(Clone, Copy)]
pub struct Presented<'a> {
    pub tree: &'a SceneTree,
    pub now: Duration,
}

impl SceneTree {
    #[must_use]
    pub fn at(&self, now: Duration) -> Presented<'_> {
        Presented { tree: self, now }
    }
}

impl<'a> Presented<'a> {
    #[must_use]
    pub fn paint_order(self) -> PaintOrder<'a> {
        PaintOrder::new(&self.tree.namespaces, self.now)
    }

    /// The commands of every leaf in paint order, without the groups around them.
    pub fn leaf_commands(self) -> impl Iterator<Item = &'a DrawCommand> {
        (self.paint_order().leaves()).flat_map(|leaf| leaf.packet.commands().iter())
    }
}

impl<'a> PaintOrder<'a> {
    pub(super) fn new(namespaces: &'a BTreeMap<NamespaceId, Namespace>, now: Duration) -> Self {
        let mut order = Self {
            namespaces,
            now,
            levels: Vec::new(),
        };
        let root = order.level(SceneTree::ROOT, Affine::IDENTITY);
        order.levels.extend(root);
        order
    }

    pub fn walk<V: PaintVisitor<'a>>(mut self, visitor: &mut V) {
        let mut open = Vec::new();
        while let Some(step) = self.next_step() {
            match step {
                Step::Group { key, values, world } => open.push(visitor.group(key, values, world)),
                Step::Graft { key, namespace } => open.push(visitor.graft(key, namespace)),
                Step::Leaf(leaf) => visitor.leaf(leaf),
                Step::Exit => {
                    if let Some(saved) = open.pop() {
                        visitor.exit(saved);
                    }
                }
            }
        }
    }

    pub fn leaves(mut self) -> impl Iterator<Item = PlacedLeaf<'a>> {
        std::iter::from_fn(move || {
            loop {
                if let Step::Leaf(leaf) = self.next_step()? {
                    return Some(leaf);
                }
            }
        })
    }

    /// The level for the top nodes of `namespace`, or `None` when the tree has no such namespace. A missing namespace
    /// paints nothing.
    fn level(&self, namespace: NamespaceId, world: Affine) -> Option<Level<'a>> {
        let entered = self.namespaces.get(&namespace)?;
        Some(Level {
            namespace,
            nodes: &entered.nodes,
            children: entered.top.iter(),
            world,
        })
    }

    fn next_step(&mut self) -> Option<Step<'a>> {
        loop {
            let level = self.levels.last_mut()?;
            let Some(id) = level.children.next() else {
                let _exhausted_level = self.levels.pop();
                // No `Group` or `Graft` step opened the root namespace, so leaving it yields no `Exit`.
                return (!self.levels.is_empty()).then_some(Step::Exit);
            };
            let key = NodeKey::new(level.namespace, *id);
            let nodes = level.nodes;
            if let Some(step) = nodes.get(id).and_then(|node| self.step(key, node)) {
                return Some(step);
            }
        }
    }

    fn step(&mut self, key: NodeKey, node: &'a StoredNode) -> Option<Step<'a>> {
        let level = self.levels.last()?;
        let (nodes, world) = (level.nodes, level.world);
        match &node.kind {
            StoredKind::Group { group, children } => {
                let values = group.at(self.now);
                let world = world.then(values.transform);
                self.levels.push(Level {
                    namespace: key.namespace,
                    nodes,
                    children: children.iter(),
                    world,
                });
                Some(Step::Group { key, values, world })
            }
            StoredKind::Leaf(Content::Packet(packet)) => {
                Some(Step::Leaf(PlacedLeaf { key, packet, world }))
            }
            StoredKind::Leaf(Content::Graft(grafted)) => {
                let entered = self.level(*grafted, world)?;
                self.levels.push(entered);
                Some(Step::Graft {
                    key,
                    namespace: *grafted,
                })
            }
        }
    }
}
