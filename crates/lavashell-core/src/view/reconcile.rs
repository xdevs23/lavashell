// SPDX-License-Identifier: GPL-3.0-only
use super::View;
use super::failure::{Failed, Repaired};
use super::frame_error::ComponentError;
use super::laid_out::{LaidOutTree, NodeLocations};
use super::transition::Departure;
use crate::ui::{Node, NodeId};
use std::cell::OnceCell;
use std::collections::BTreeSet;

struct PaintedIn {
    parent: NodeId,
    inside: usize,
}

impl View {
    pub(super) fn replace_tree(&mut self, nodes: Vec<Node>) {
        let repaired = self.repaired(nodes, BTreeSet::new());
        let failed_exits = self.reconcile(repaired);
        let _whether_nodes_were_lost_is_not_needed = self.fail(failed_exits);
    }

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
            for &id in self.painting.sibling_orders.children(parent) {
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

    fn painted_inside(&self, root: NodeId) -> Vec<NodeId> {
        let (mut leaving, mut painted) = (vec![root], Vec::new());
        while let Some(id) = leaving.pop() {
            for node in self.transitions.subtree(id) {
                let children = self.painting.sibling_orders.children(node).iter().copied();
                leaving.extend(children.filter(|&child| self.transitions.is_leaving(child)));
                painted.push(node);
            }
        }
        painted
    }

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
            self.painting.sibling_orders.remove(id);
            let _scroll_of_a_removed_node = self.scrolls.remove(&id);
            let _edit_of_a_removed_node = self.editing.edits.remove(&id);
            let _text_geometry_of_a_removed_node = self.editing.positions.remove(&id);
            let _visuals_of_a_removed_node = self.painting.interaction_visuals.remove(&id);
        }
        memory.retain(|(node, _)| !gone.contains(node));
    }
}
