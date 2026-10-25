// SPDX-License-Identifier: GPL-3.0-only
use crate::ui::Node;

/// The parent of each node of a composed tree.
#[derive(Clone, Default)]
pub(super) struct Parents(Vec<Option<Parent>>);

/// The parent of a node, with the position of the node among its children.
#[derive(Clone, Copy)]
pub(super) struct Parent {
    pub node: usize,
    pub position: usize,
}

impl Parents {
    pub fn new(nodes: &[Node]) -> Self {
        let mut parents = vec![None; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            for (position, &child) in node.children.iter().enumerate() {
                parents[child] = Some(Parent {
                    node: index,
                    position,
                });
            }
        }
        Self(parents)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn of(&self, child: usize) -> Option<Parent> {
        self.0[child]
    }

    pub fn node_of(&self, child: usize) -> Option<usize> {
        self.0[child].map(|parent| parent.node)
    }

    /// The node at `index` and its ancestors, up to the root.
    pub fn ancestry(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(index), |&node| self.node_of(node))
    }

    /// The parent of the node at `index`, then the parent of that parent, up to the root.
    pub fn chain(&self, index: usize) -> Vec<Parent> {
        std::iter::successors(self.of(index), |parent| self.of(parent.node)).collect()
    }
}
