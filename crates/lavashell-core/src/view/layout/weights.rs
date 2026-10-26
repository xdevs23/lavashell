// SPDX-License-Identifier: GPL-3.0-only
use super::Measuring;
use crate::geometry::{Axis, Constraints, Size, Span};
use crate::ui::{Arrangement, Flow, MainLength, Node, NodeKind, Weight};
use crate::units::{Dp, Positive};
use crate::view::TextMeasurer;
use crate::view::parents::Parents;

/// The weight given to a child that fills along the main axis of a stack. The child gets at most its share of the
/// free length. A bound set in the modifier before the fill therefore still limits the child.
const FILLING_WEIGHT: Weight = Weight::UpTo(crate::constant!(Positive::try_new(1.0)));

/// The weight with which `child` gets a share of the length of the row or the column `container`, or `None` for a
/// child that gets no share. No child gets a share while `bounded` is false, because the length of the container
/// is then unbounded.
pub(in crate::view) fn weight(
    nodes: &[Node],
    container: usize,
    child: usize,
    bounded: bool,
) -> Option<Weight> {
    let NodeKind::Container(Arrangement::Along(axis)) = nodes[container].kind else {
        return None;
    };
    let (true, Flow::In(weight)) = (bounded, nodes[child].flow) else {
        return None;
    };
    let scrolls = nodes[container].scroll_axis().is_some();
    let fills = || !scrolls && fills(nodes, child, axis, &mut |_| true);
    weight.or_else(|| fills().then_some(FILLING_WEIGHT))
}

/// Whether the child `child` of the row or the column `container` fills only through the children of an overlay.
/// The conditions of those children decide during layout whether the child gets a share.
pub(in crate::view) fn fills_through_children(
    nodes: &[Node],
    container: usize,
    child: usize,
) -> bool {
    let NodeKind::Container(Arrangement::Along(axis)) = nodes[container].kind else {
        return false;
    };
    let node = &nodes[child];
    matches!(node.flow, Flow::In(None))
        && node.main_length(axis) == MainLength::Content
        && nodes[container].scroll_axis().is_none()
        && fills(nodes, child, axis, &mut |_| true)
}

/// The child of a row or a column that fills only through the children of an overlay, when the condition of `node`
/// decides whether that child fills. The row or the column resolves such a condition before it measures the child.
pub(in crate::view) fn fill_decided_by(
    nodes: &[Node],
    parents: &Parents,
    node: usize,
) -> Option<usize> {
    let (mut child, mut below) = (node, Vec::new());
    loop {
        let parent = parents.node_of(child)?;
        match nodes[parent].kind {
            NodeKind::Container(Arrangement::Overlay) => {
                below.push(child);
                child = parent;
            }
            NodeKind::Container(Arrangement::Along(axis)) => {
                let fills_on_the_way = |inside: &usize| {
                    matches!(nodes[*inside].flow, Flow::In(_))
                        && fills(nodes, *inside, axis, &mut |_| true)
                };
                let content =
                    |above: &usize| nodes[*above].main_length(axis) == MainLength::Content;
                let decides = !below.is_empty()
                    && fills_through_children(nodes, parent, child)
                    && below.iter().all(fills_on_the_way)
                    && below[1..].iter().all(content);
                return decides.then_some(child);
            }
            _ => return None,
        }
    }
}

/// Whether the node at `node` fills the length that its parent proposes along `axis`. An overlay fills through its
/// children in the flow for which `shown` is true.
fn fills(nodes: &[Node], node: usize, axis: Axis, shown: &mut impl FnMut(usize) -> bool) -> bool {
    let node = &nodes[node];
    match node.main_length(axis) {
        MainLength::Fills => true,
        MainLength::Set => false,
        MainLength::Content => {
            matches!(node.kind, NodeKind::Container(Arrangement::Overlay))
                && (node.children.iter()).any(|&child| {
                    matches!(nodes[child].flow, Flow::In(_))
                        && fills(nodes, child, axis, &mut *shown)
                        && shown(child)
                })
        }
    }
}

/// The weighted children of a stack, each with its position in the stack, its node index and its weight, and the sum
/// of their weight factors.
pub(super) struct WeightedChildren {
    pub members: Vec<(usize, usize, Weight)>,
    pub total: f64,
}

impl WeightedChildren {
    pub fn share(&self, available: Dp, weight: Weight) -> Dp {
        available * (weight.factor() / self.total)
    }

    /// The smallest length to share for a child of `weight` to get at least `length`.
    fn needed(&self, length: Dp, weight: Weight) -> Dp {
        let mut needed = length * (self.total / weight.factor());
        while self.share(needed, weight) < length {
            needed = Dp::new(needed.get().next_up());
        }
        needed
    }
}

impl<T: TextMeasurer> Measuring<'_, T> {
    /// Whether the child with a weight `child` of the stack `container` takes a share once the conditions are
    /// resolved. A child that fills only through the children of an overlay takes none while each of those children
    /// is hidden.
    pub(super) fn takes_share(&mut self, container: usize, child: usize) -> bool {
        let nodes = self.nodes;
        let NodeKind::Container(Arrangement::Along(axis)) = nodes[container].kind else {
            return true;
        };
        let mut shown = |node| self.resolve_conditions(node);
        !fills_through_children(nodes, container, child) || fills(nodes, child, axis, &mut shown)
    }

    /// Measures the children with a weight at their shares of the length that `fixed` leaves along the main axis of
    /// `c`, and stores the sizes in `sizes`. Returns the smallest length to share at which every child with a weight
    /// gets its measured length, or `None` for a stack without children with a weight. Where the main axis allows more
    /// than it proposes, the children share at least the length that they need at their smallest.
    pub(super) fn measure_shares(
        &mut self,
        weighted: &WeightedChildren,
        sizes: &mut [Size],
        c: Constraints,
        fixed: Dp,
        vertical: bool,
    ) -> Option<Dp> {
        let main = if vertical { c.height } else { c.width };
        let length = |size: Size| if vertical { size.height } else { size.width };
        let weighted_constraints = |span: Span| {
            if vertical {
                Constraints {
                    width: c.width.loosened(),
                    height: span,
                }
            } else {
                Constraints {
                    width: span,
                    height: c.height.loosened(),
                }
            }
        };
        let mut available = (main.proposed() - fixed).max(Dp::ZERO);
        if main.allows_more() {
            let at_smallest = self.measure_weighted(weighted, sizes, length, |_| {
                weighted_constraints(Span::offering(Dp::ZERO))
            });
            let bounded = main.clamp(fixed + at_smallest.unwrap_or_default()) - fixed;
            available = available.max(bounded);
        }
        self.measure_weighted(weighted, sizes, length, |weight| {
            let share = weighted.share(available, weight);
            weighted_constraints(match weight {
                Weight::Fill(_) => Span::exact(share),
                Weight::UpTo(_) => Span::up_to(share),
            })
        })
    }

    /// Measures each weighted child under the constraints returned by `constraints` for its weight, and stores the
    /// sizes in `sizes`. Returns the smallest length to share at which every weighted child gets the `length` of its
    /// measured size, or `None` for a stack without weighted children.
    fn measure_weighted(
        &mut self,
        weighted: &WeightedChildren,
        sizes: &mut [Size],
        length: impl Fn(Size) -> Dp,
        constraints: impl Fn(Weight) -> Constraints,
    ) -> Option<Dp> {
        (weighted.members.iter())
            .map(|&(p, child, weight)| {
                sizes[p] = self.measure(child, 0, constraints(weight));
                weighted.needed(length(sizes[p]), weight)
            })
            .reduce(Dp::max)
    }
}
