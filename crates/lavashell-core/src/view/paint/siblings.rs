// SPDX-License-Identifier: GPL-3.0-only
use super::{BTreeMap, BTreeSet, Inherited, NodeId, Point, TextMeasurer, Transform, View};
use crate::view::shown_children;

#[derive(Clone, Copy)]
enum Slot {
    Live(usize),
    Leaving(NodeId),
}

#[derive(Clone, Copy)]
struct Sibling {
    position: usize,
    id: NodeId,
    can_exit: bool,
}

/// The children of each parent, in the order in which they were painted last, leaving children
/// included.
#[derive(Default)]
pub(in crate::view) struct SiblingOrders(BTreeMap<NodeId, Vec<NodeId>>);

impl SiblingOrders {
    pub fn remove(&mut self, parent: NodeId) {
        let _order_of_a_removed_parent = self.0.remove(&parent);
    }

    /// The children of `parent`, in the order in which they were painted last.
    pub fn children(&self, parent: NodeId) -> &[NodeId] {
        self.0.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// The paint order of `children` under `parent`, or `None` when no child can exit and no child painted last time
    /// is leaving. A leaving child paints right after the sibling that it followed in the last paint.
    fn order(
        &mut self,
        parent: NodeId,
        children: impl Iterator<Item = Sibling> + Clone,
        is_leaving: impl Fn(NodeId) -> bool,
    ) -> Option<Vec<Slot>> {
        let previous = self.0.remove(&parent).unwrap_or_default();
        let can_exit = children.clone().any(|child| child.can_exit);
        if !can_exit && !previous.iter().any(|id| is_leaving(*id)) {
            return None;
        }
        let present: BTreeSet<NodeId> = children.clone().map(|child| child.id).collect();
        let mut followers = BTreeMap::new();
        let mut anchor = None;
        for &id in &previous {
            if present.contains(&id) {
                anchor = Some(id);
            } else if is_leaving(id) {
                let _no_other_follower_of_the_anchor = followers.insert(anchor, id);
                anchor = Some(id);
            }
        }
        let (mut slots, mut painted) = (Vec::new(), Vec::new());
        let ids = children.map(|child| Some((child.position, child.id)));
        for start in std::iter::once(None).chain(ids) {
            let mut anchor = None;
            if let Some((child, id)) = start {
                slots.push(Slot::Live(child));
                painted.push(id);
                anchor = Some(id);
            }
            while let Some(id) = followers.remove(&anchor) {
                slots.push(Slot::Leaving(id));
                painted.push(id);
                anchor = Some(id);
            }
        }
        let _order_of_the_previous_paint = self.0.insert(parent, painted);
        Some(slots)
    }

    pub fn leaving_under(
        &self,
        parents: impl Iterator<Item = NodeId>,
        is_leaving: impl Fn(NodeId) -> bool,
    ) -> Vec<NodeId> {
        parents
            .filter_map(|parent| self.0.get(&parent))
            .flatten()
            .copied()
            .filter(|id| is_leaving(*id))
            .collect()
    }
}

impl View {
    /// Paints the shown children of the node at `index`, and each leaving child right after the sibling that it
    /// followed in the last paint.
    pub(super) fn paint_children(
        &mut self,
        index: usize,
        pos: Point,
        transform: Transform,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        if let Some(order) = self.sibling_order(index) {
            for slot in order {
                match slot {
                    Slot::Live(child) => {
                        self.paint_child(index, child, pos, transform, inherited, text);
                    }
                    Slot::Leaving(leaving) => {
                        self.paint_departure(leaving, pos, transform, inherited, text);
                    }
                }
            }
        } else {
            let shown: Vec<_> = shown_children(&self.nodes, &self.layout, index).collect();
            for (child, _) in shown {
                self.paint_child(index, child, pos, transform, inherited, text);
            }
        }
    }

    fn sibling_order(&mut self, index: usize) -> Option<Vec<Slot>> {
        let nodes = &self.nodes;
        let shown = shown_children(nodes, &self.layout, index);
        let children = shown.map(|(position, child)| Sibling {
            position,
            id: nodes[child].id,
            can_exit: nodes[child].transition.is_some_and(|t| t.exits()),
        });
        let transitions = &self.transitions;
        self.sibling_orders
            .order(nodes[index].id, children, |id| transitions.is_leaving(id))
    }

    fn paint_child(
        &mut self,
        index: usize,
        child: usize,
        pos: Point,
        transform: Transform,
        inherited: Inherited,
        text: &mut impl TextMeasurer,
    ) {
        let offset = self.layout[index].children[child];
        let position = Point::new(pos.x + offset.x, pos.y + offset.y);
        self.paint(
            self.nodes[index].children[child],
            position,
            pos,
            transform,
            inherited,
            text,
        );
    }
}
