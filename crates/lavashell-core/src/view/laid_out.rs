// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::parents::Parents;
use crate::geometry::Point;
use crate::ui::{Node, NodeId};
use crate::view::layout::{Layout, shown_children};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(super) struct LaidOutTree {
    nodes: Vec<Node>,
    layout: Vec<Layout>,
    parents: Parents,
}

impl LaidOutTree {
    pub fn replaced(view: &mut View, nodes: Vec<Node>, parents: Parents) -> Self {
        let mut tree = Self {
            layout: vec![Layout::default(); nodes.len()],
            nodes,
            parents,
        };
        tree.exchange(view);
        tree
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn parents(&self) -> &Parents {
        &self.parents
    }

    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().map(|node| node.id)
    }

    pub fn exchange(&mut self, view: &mut View) {
        std::mem::swap(&mut self.nodes, &mut view.nodes);
        std::mem::swap(&mut self.layout, &mut view.layout);
        std::mem::swap(&mut self.parents, &mut view.parents);
    }

    pub fn subtree(&self, index: usize, skipped: impl Fn(NodeId) -> bool) -> Self {
        let mut tree = Self::default();
        tree.append(self, index, &skipped);
        tree.parents = Parents::new(&tree.nodes);
        tree
    }

    fn append(&mut self, source: &Self, index: usize, skipped: &impl Fn(NodeId) -> bool) {
        let appended = self.nodes.len();
        let mut node = source.nodes[index].clone();
        node.children.clear();
        self.nodes.push(node);
        self.layout.push(Layout {
            children: Vec::new(),
            ..source.layout[index].clone()
        });
        let children = source.nodes[index].children.iter();
        for (&child, &offset) in children.zip(&source.layout[index].children) {
            if !skipped(source.nodes[child].id) {
                let appended_child = self.nodes.len();
                self.append(source, child, skipped);
                self.nodes[appended].children.push(appended_child);
                self.layout[appended].children.push(offset);
            }
        }
    }
}

pub(super) struct NodeLocations<'a>(BTreeMap<NodeId, (&'a LaidOutTree, usize, Point)>);

impl<'a> NodeLocations<'a> {
    pub fn new(trees: impl IntoIterator<Item = &'a LaidOutTree>) -> Self {
        let mut locations = BTreeMap::new();
        for tree in trees {
            for (index, layout) in tree.layout.iter().enumerate() {
                for (position, child) in shown_children(&tree.nodes, &tree.layout, index) {
                    if let Some(&offset) = layout.children.get(position) {
                        let _location_in_an_earlier_tree =
                            locations.insert(tree.nodes[child].id, (tree, child, offset));
                    }
                }
            }
        }
        Self(locations)
    }

    pub fn subtree(&self, id: NodeId) -> Option<(LaidOutTree, Point)> {
        let &(tree, index, offset) = self.0.get(&id)?;
        Some((tree.subtree(index, |_| false), offset))
    }
}
