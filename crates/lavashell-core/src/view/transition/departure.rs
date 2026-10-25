// SPDX-License-Identifier: GPL-3.0-only
use super::{BTreeSet, NodeId, Point};
use crate::animation::AnimationError;
use crate::view::laid_out::LaidOutTree;

/// A removed node and its subtree, as they were last composed and laid out. They paint until the
/// exit animation of the node ends.
pub(in crate::view) struct Departure {
    tree: LaidOutTree,
    /// Where the node was laid out, relative to the content area of the parent node.
    offset: Point,
}

impl Departure {
    pub fn new(tree: LaidOutTree, offset: Point) -> Self {
        Self { tree, offset }
    }

    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.tree.ids()
    }

    /// Removes the subtrees of the nodes in `present` from the departure. Those nodes are composed
    /// again and paint at their new place in the tree.
    pub fn detach(&mut self, present: &BTreeSet<NodeId>) {
        if self.tree.ids().any(|id| present.contains(&id)) {
            self.tree = self.tree.subtree(0, |id| present.contains(&id));
        }
    }

    /// Moves the animations made with `Ui::animate_presence` in the subtree to their exit values.
    pub(super) fn run_exit_actions(&self) -> Result<(), AnimationError> {
        for exit in self.tree.nodes().iter().flat_map(|node| &node.on_exit) {
            exit()?;
        }
        Ok(())
    }

    pub fn position(&self, origin: Point) -> Point {
        origin.offset(self.offset.x, self.offset.y)
    }

    pub fn tree(&self) -> &LaidOutTree {
        &self.tree
    }

    pub fn take_tree(&mut self) -> LaidOutTree {
        std::mem::take(&mut self.tree)
    }

    pub fn restore_tree(&mut self, tree: LaidOutTree) {
        self.tree = tree;
    }

    pub fn into_tree(self) -> LaidOutTree {
        self.tree
    }
}
