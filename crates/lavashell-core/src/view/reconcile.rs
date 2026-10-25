// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::{LayoutError, NodeName};
use super::graph::Graph;
use super::laid_out::{LaidOutTree, NodeLocations};
use super::parents::Parents;
use super::transition::Departure;
use super::{BTreeSet, Node, NodeId, View};
use crate::animation::AnimationError;
use std::cell::OnceCell;

impl View {
    /// Replaces the composed nodes with `nodes` and resets the layout. Starts the exits of the removed nodes and drops
    /// the state of the nodes that no longer paint. Fails without a change when two nodes have the same id or when
    /// `nodes` cannot be laid out. Fails after the replacement when the value or the velocity of an exit animation of a
    /// removed node is not finite.
    pub(super) fn reconcile(&mut self, nodes: Vec<Node>) -> Result<(), LayoutError> {
        let mut present = BTreeSet::new();
        if let Some(repeated) = nodes.iter().find(|node| !present.insert(node.id)) {
            return Err(LayoutError::DuplicateKey(NodeName::of(repeated)));
        }
        let parents = Parents::new(&nodes);
        let graph = Graph::new(&nodes, &parents)?;
        let previous = LaidOutTree::replaced(self, nodes, parents);
        self.recover_focus(&previous);
        self.graph = graph;
        self.end_captures(|id| !present.contains(&id));
        let leaving: Vec<_> = self
            .transitions
            .departures()
            .map(|d| d.tree().clone())
            .collect();
        let mut gone: BTreeSet<_> = (previous.ids())
            .chain(leaving.iter().flat_map(LaidOutTree::ids))
            .collect();
        let painted = self
            .start_exits(&present, &previous, &leaving)
            .map_err(LayoutError::Exit)?;
        gone.retain(|id| !painted.contains(id));
        self.forget(&gone);
        self.retire_exits();
        Ok(())
    }

    /// Puts the leaving nodes that are in `present` back into the tree. Starts the exit of every other node with an
    /// exit transition that was painted among the children of a node that paints now. The departure of such a node
    /// copies its subtree from the tree where the node was laid out last: `previous`, a tree in `leaving` from before
    /// this composition, or the tree of a node that came back. Returns the ids of the nodes that paint.
    fn start_exits(
        &mut self,
        present: &BTreeSet<NodeId>,
        previous: &LaidOutTree,
        leaving: &[LaidOutTree],
    ) -> Result<BTreeSet<NodeId>, AnimationError> {
        let returned: Vec<_> = (self.transitions.return_to_tree(present).into_iter())
            .map(Departure::into_tree)
            .collect();
        let locations = OnceCell::new();
        let mut parents: Vec<NodeId> = present.iter().copied().collect();
        let mut painted = present.clone();
        while let Some(parent) = parents.pop() {
            for &id in self.sibling_orders.children(parent) {
                if painted.contains(&id) {
                    continue;
                }
                let trees = || std::iter::once(previous).chain(leaving).chain(&returned);
                if self.transitions.exits_on_removal(id)
                    && let Some((tree, offset)) = locations
                        .get_or_init(|| NodeLocations::new(trees()))
                        .subtree(id)
                {
                    let mut departure = Departure::new(tree, offset);
                    departure.detach(present);
                    self.transitions.leave(id, departure)?;
                } else if let Some(departure) = self.transitions.departure(id) {
                    departure.detach(present);
                }
                let inside = self.painted_inside(id);
                parents.extend(&inside);
                painted.extend(inside);
            }
        }
        Ok(painted)
    }

    /// The ids of the nodes that paint inside the leaving node `root`: the nodes in its departure, and the nodes in the
    /// departures of the leaving nodes that the last paint placed among the children of those nodes.
    fn painted_inside(&self, root: NodeId) -> Vec<NodeId> {
        let (mut leaving, mut painted) = (vec![root], Vec::new());
        while let Some(id) = leaving.pop() {
            for node in self.transitions.subtree(id) {
                let children = self.sibling_orders.children(node).iter().copied();
                leaving.extend(children.filter(|&child| self.transitions.is_leaving(child)));
                painted.push(node);
            }
        }
        painted
    }

    /// Drops the departures of the leaving nodes whose exits ended, with the departures painted
    /// inside them.
    pub(super) fn retire_exits(&mut self) {
        let gone: BTreeSet<_> = (self.transitions.ended_exits().into_iter())
            .flat_map(|id| self.painted_inside(id))
            .collect();
        self.forget(&gone);
    }

    fn forget(&mut self, gone: &BTreeSet<NodeId>) {
        self.retained.remove(gone);
        let mut memory = self.memory.borrow_mut();
        for &id in gone {
            self.transitions.remove(id);
            self.sibling_orders.remove(id);
            let _scroll_of_a_removed_node = self.scrolls.remove(&id);
            let _edit_of_a_removed_node = self.edits.remove(&id);
            let _text_geometry_of_a_removed_node = self.text_positions.remove(&id);
            let _visuals_of_a_removed_node = self.interaction_visuals.remove(&id);
        }
        memory.retain(|(node, _)| !gone.contains(node));
    }
}
