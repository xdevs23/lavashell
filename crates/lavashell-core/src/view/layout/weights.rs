// SPDX-License-Identifier: GPL-3.0-only
use super::Measuring;
use super::pass::MeasuredParts;
use crate::geometry::{Axis, Constraints, Size, Span};
use crate::ui::{Arrangement, Flow, MainLength, ModifierElement, Node, NodeKind, Weight};
use crate::units::{Dp, Positive};
use crate::view::TextMeasurer;
use crate::view::parents::Parents;

const FILLING_WEIGHT: Weight = Weight::UpTo(crate::constant!(Positive::try_new(1.0)));

pub(in crate::view) fn weight(
    nodes: &[Node],
    container: usize,
    child: usize,
    bounded: bool,
) -> Option<Weight> {
    let NodeKind::Container(Arrangement::Along { axis, .. }) = nodes[container].kind else {
        return None;
    };
    let (true, Flow::In(weight)) = (bounded, nodes[child].flow) else {
        return None;
    };
    let scrolls = nodes[container].scroll_axis().is_some();
    let fills = || !scrolls && fills(nodes, child, axis, &mut |_| true);
    weight.or_else(|| fills().then_some(FILLING_WEIGHT))
}

pub(in crate::view) fn fills_through_children(
    nodes: &[Node],
    container: usize,
    child: usize,
) -> bool {
    let NodeKind::Container(Arrangement::Along { axis, .. }) = nodes[container].kind else {
        return false;
    };
    let node = &nodes[child];
    matches!(node.flow, Flow::In(None))
        && node.main_length(axis) == MainLength::Content
        && nodes[container].scroll_axis().is_none()
        && fills(nodes, child, axis, &mut |_| true)
}

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
            NodeKind::Container(Arrangement::Along { axis, .. }) => {
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

fn fills(nodes: &[Node], node: usize, axis: Axis, shown: &mut impl FnMut(usize) -> bool) -> bool {
    let own = |node: &Node| match node.main_length(axis) {
        MainLength::Fills => Some(true),
        MainLength::Set => Some(false),
        MainLength::Content => None,
    };
    fills_by(nodes, node, &own, shown)
}

fn fills_share(
    nodes: &[Node],
    node: usize,
    axis: Axis,
    shown: &mut impl FnMut(usize) -> bool,
) -> bool {
    let own = |node: &Node| {
        (node.modifier.iter()).find_map(|element| match (axis, element) {
            (Axis::Horizontal, ModifierElement::FillWidth)
            | (Axis::Vertical, ModifierElement::FillHeight) => Some(true),
            (Axis::Horizontal, ModifierElement::Width(_))
            | (Axis::Vertical, ModifierElement::Height(_))
            | (_, ModifierElement::MeasurementLimit(_)) => Some(false),
            (_, ModifierElement::MaxSize(most)) => axis.length(*most).is_finite().then_some(false),
            _ => None,
        })
    };
    fills_by(nodes, node, &own, shown)
}

fn fills_by(
    nodes: &[Node],
    node: usize,
    own: &impl Fn(&Node) -> Option<bool>,
    shown: &mut impl FnMut(usize) -> bool,
) -> bool {
    let node = &nodes[node];
    own(node).unwrap_or_else(|| {
        matches!(node.kind, NodeKind::Container(Arrangement::Overlay))
            && (node.children.iter()).any(|&child| {
                matches!(nodes[child].flow, Flow::In(_))
                    && fills_by(nodes, child, own, &mut *shown)
                    && shown(child)
            })
    })
}

pub(super) struct WeightedChildren {
    pub members: Vec<(usize, usize, Weight)>,
    pub total: f64,
}

impl WeightedChildren {
    pub fn share(&self, available: Dp, weight: Weight) -> Dp {
        available * (weight.factor() / self.total)
    }

    fn needed(&self, length: Dp, weight: Weight) -> Dp {
        let mut needed = length * (self.total / weight.factor());
        while self.share(needed, weight) < length {
            needed = Dp::new(needed.get().next_up());
        }
        needed
    }

    fn needed_for(&self, lengths: impl Iterator<Item = Dp>) -> Option<Dp> {
        (self.members.iter().zip(lengths))
            .map(|(&(.., weight), length)| self.needed(length, weight))
            .reduce(Dp::max)
    }
}

#[derive(Clone, Copy)]
enum Smallest {
    Size(Size),
    Length(Dp),
}

impl Smallest {
    fn length(self, axis: Axis) -> Dp {
        match self {
            Self::Size(size) => axis.length(size),
            Self::Length(length) => length,
        }
    }
}

impl<T: TextMeasurer> Measuring<'_, T> {
    pub(super) fn takes_share(&mut self, container: usize, child: usize) -> bool {
        let nodes = self.nodes;
        let NodeKind::Container(Arrangement::Along { axis, .. }) = nodes[container].kind else {
            return true;
        };
        let mut shown = |node| self.resolve_conditions(node);
        !fills_through_children(nodes, container, child) || fills(nodes, child, axis, &mut shown)
    }

    pub(super) fn measure_shares(
        &mut self,
        weighted: &WeightedChildren,
        sizes: &mut [Size],
        c: Constraints,
        fixed: Dp,
        axis: Axis,
    ) -> Option<Dp> {
        let main = axis.span(c);
        let loose = Constraints {
            width: c.width.loosened(),
            height: c.height.loosened(),
        };
        let mut available = (main.proposed() - fixed).max(Dp::ZERO);
        let smallest = (main.allows_more()).then(|| self.measure_smallest(weighted, loose, axis));
        if let Some(found) = &smallest {
            let needed = weighted.needed_for(found.iter().map(|child| child.length(axis)));
            let bounded = main.clamp(fixed + needed.unwrap_or_default()) - fixed;
            available = available.max(bounded);
        }
        let smallest = smallest.as_deref();
        self.measure_at_shares(weighted, sizes, loose, available, axis, smallest)
    }

    fn measure_smallest(
        &mut self,
        weighted: &WeightedChildren,
        loose: Constraints,
        axis: Axis,
    ) -> Vec<Smallest> {
        let at_smallest_length = axis.with_span(loose, Span::offering(Dp::ZERO));
        let width_alone = Constraints {
            width: Span::offering(Dp::ZERO),
            height: Span::offering(Dp::INFINITY),
        };
        (weighted.members.iter())
            .map(|&(_, child, _)| {
                if axis == Axis::Horizontal
                    && !self.graph.has_read_within_parent(child)
                    && let Some(width) = self.measure_width_alone(child, width_alone)
                {
                    Smallest::Length(width)
                } else {
                    Smallest::Size(self.measure(child, 0, at_smallest_length))
                }
            })
            .collect()
    }

    fn measure_at_shares(
        &mut self,
        weighted: &WeightedChildren,
        sizes: &mut [Size],
        loose: Constraints,
        available: Dp,
        axis: Axis,
        smallest: Option<&[Smallest]>,
    ) -> Option<Dp> {
        for (m, &(p, child, weight)) in weighted.members.iter().enumerate() {
            let share = weighted.share(available, weight);
            let at_smallest = smallest.and_then(|found| found.get(m).copied());
            let known = self.size_at_share(child, weight, axis, share, at_smallest);
            sizes[p] = known.unwrap_or_else(|| {
                let span = match weight {
                    Weight::Fill(_) => Span::exact(share),
                    Weight::UpTo(_) => Span::up_to(share),
                };
                self.measure(child, 0, axis.with_span(loose, span))
            });
        }
        let lengths = (weighted.members.iter()).map(|&(p, ..)| axis.length(sizes[p]));
        weighted.needed_for(lengths)
    }

    fn size_at_share(
        &mut self,
        child: usize,
        weight: Weight,
        axis: Axis,
        share: Dp,
        smallest: Option<Smallest>,
    ) -> Option<Size> {
        let parts = self.pass.probe_parts()?;
        if self.graph.has_read_within_parent(child) {
            return None;
        }
        let known = match (axis, parts, smallest) {
            (Axis::Vertical, _, Some(Smallest::Size(size)))
                if !self.graph.holds_read_source(child) =>
            {
                axis.with_length(size, share)
            }
            (Axis::Horizontal, MeasuredParts::WidthOnly, _) => Size::new(share, Dp::ZERO),
            _ => return None,
        };
        self.takes_whole_share(child, weight, axis).then_some(known)
    }

    fn takes_whole_share(&mut self, child: usize, weight: Weight, axis: Axis) -> bool {
        let nodes = self.nodes;
        let mut shown = |node| self.resolve_conditions(node);
        matches!(weight, Weight::Fill(_)) || fills_share(nodes, child, axis, &mut shown)
    }
}
