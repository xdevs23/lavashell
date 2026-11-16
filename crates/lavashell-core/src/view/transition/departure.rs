// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::AnimationError;
use crate::geometry::Point;
use crate::ui::NodeId;
use crate::view::laid_out::LaidOutTree;
use std::collections::BTreeSet;

pub(in crate::view) struct Departure {
    tree: LaidOutTree,
    offset: Point,
}

impl Departure {
    pub fn new(tree: LaidOutTree, offset: Point) -> Self {
        Self { tree, offset }
    }

    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.tree.ids()
    }

    pub fn detach(&mut self, present: &BTreeSet<NodeId>) {
        if self.tree.ids().any(|id| present.contains(&id)) {
            self.tree = self.tree.subtree(0, |id| present.contains(&id));
        }
    }

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
