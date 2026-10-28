// SPDX-License-Identifier: GPL-3.0-only
use super::failure::{Failed, Repaired};
use super::frame_error::ComponentError;
use super::laid_out::{LaidOutTree, NodeLocations};
use super::transition::Departure;
use super::{BTreeSet, Node, NodeId, View};
use std::cell::OnceCell;

/// A node whose children paint, with the index of the present node inside which the node paints.
struct PaintedIn {
    parent: NodeId,
    inside: usize,
}

impl View {
    /// Replaces the composed nodes with `nodes`. A composition fails, and loses its contents, where the tree cannot be
    /// laid out or where a node removed by the composition cannot start its exit.
    pub(super) fn replace_tree(&mut self, nodes: Vec<Node>) {
        let repaired = self.repaired(nodes, BTreeSet::new());
        let failed_exits = self.reconcile(repaired);
        let _lost_nodes = self.fail(failed_exits);
    }

    /// Replaces the composed nodes with the nodes of `tree` and resets the layout. Starts the exits of the removed
    /// nodes, apart from the nodes dropped with the contents of failed compositions, and drops the state of the nodes
    /// that no longer paint. Returns a failure for each removed node whose exit cannot start.
    pub(super) fn reconcile(&mut self, tree: Repaired) -> Vec<Failed> {
        let Repaired {
            nodes,
            parents,
            graph,
            dropped,
        } = tree;
        let present: BTreeSet<_> = nodes.iter().map(|node| node.id).collect();
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
        let (painted, failed_exits) = self.start_exits(&present, &dropped, &previous, &leaving);
        gone.retain(|id| !painted.contains(id));
        self.forget(&gone);
        self.retire_exits();
        failed_exits
    }

    /// Puts the leaving nodes that are in `present` back into the tree. Starts the exit of every other node with an
    /// exit transition that was painted among the children of a node that paints now, unless the node is in
    /// `dropped`. The departure of such a node copies its subtree from the tree where the node was laid out last:
    /// `previous`, a tree in `leaving` from before this composition, or the tree of a node that came back. Returns the
    /// ids of the nodes that paint, and a failure inside the present node among whose children each exit that cannot
    /// start was painted.
    fn start_exits(
        &mut self,
        present: &BTreeSet<NodeId>,
        dropped: &BTreeSet<NodeId>,
        previous: &LaidOutTree,
        leaving: &[LaidOutTree],
    ) -> (BTreeSet<NodeId>, Vec<Failed>) {
        let returned: Vec<_> = (self.transitions.return_to_tree(present).into_iter())
            .map(Departure::into_tree)
            .collect();
        let locations = OnceCell::new();
        let mut parents: Vec<_> = (self.nodes.iter().enumerate())
            .map(|(inside, node)| PaintedIn {
                parent: node.id,
                inside,
            })
            .collect();
        let mut painted = present.clone();
        let mut failed = Vec::new();
        while let Some(PaintedIn { parent, inside }) = parents.pop() {
            for &id in self.sibling_orders.children(parent) {
                if painted.contains(&id) {
                    continue;
                }
                let trees = || std::iter::once(previous).chain(leaving).chain(&returned);
                if !dropped.contains(&id)
                    && self.transitions.exits_on_removal(id)
                    && let Some((tree, offset)) = locations
                        .get_or_init(|| NodeLocations::new(trees()))
                        .subtree(id)
                {
                    let mut departure = Departure::new(tree, offset);
                    departure.detach(present);
                    if let Err(error) = self.transitions.leave(id, departure) {
                        failed.push(Failed {
                            inside,
                            cause: ComponentError::Exit(error),
                        });
                    }
                } else if let Some(departure) = self.transitions.departure(id) {
                    departure.detach(present);
                }
                let painted_inside = self.painted_inside(id);
                parents.extend(painted_inside.iter().map(|&node| PaintedIn {
                    parent: node,
                    inside,
                }));
                painted.extend(painted_inside);
            }
        }
        (painted, failed)
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
