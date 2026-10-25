// SPDX-License-Identifier: GPL-3.0-only
use super::Measuring;
use crate::geometry::{Axis, Constraints, Size};
use crate::ui::{Arrangement, Flow, MainLength, Node, NodeKind, Weight};
use crate::units::{Dp, Positive};
use crate::view::TextMeasurer;

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
    let fills = || nodes[container].scroll_axis().is_none() && fills(nodes, child, axis);
    weight.or_else(|| fills().then_some(FILLING_WEIGHT))
}

/// Whether the node at `node` fills the length that its parent proposes along `axis`.
fn fills(nodes: &[Node], node: usize, axis: Axis) -> bool {
    let node = &nodes[node];
    match node.main_length(axis) {
        MainLength::Fills => true,
        MainLength::Set => false,
        MainLength::Content => {
            matches!(node.kind, NodeKind::Container(Arrangement::Overlay))
                && (node.children.iter()).any(|&child| {
                    matches!(nodes[child].flow, Flow::In(_)) && fills(nodes, child, axis)
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
    /// Measures each weighted child under the constraints returned by `constraints` for its weight, and stores the
    /// sizes in `sizes`. Returns the smallest length to share at which every weighted child gets the `length` of its
    /// measured size, or `None` for a stack without weighted children.
    pub(super) fn measure_weighted(
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
