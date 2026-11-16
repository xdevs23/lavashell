// SPDX-License-Identifier: GPL-3.0-only
use crate::ui::Node;

pub(super) const ROOT: usize = 0;

#[derive(Clone, Default)]
pub(super) struct Parents(Vec<Option<Parent>>);

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

    pub fn around(&self, index: usize) -> usize {
        self.node_of(index).unwrap_or(ROOT)
    }

    pub fn ancestry(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(index), |&node| self.node_of(node))
    }

    pub fn chain(&self, index: usize) -> Vec<Parent> {
        std::iter::successors(self.of(index), |parent| self.of(parent.node)).collect()
    }
}
