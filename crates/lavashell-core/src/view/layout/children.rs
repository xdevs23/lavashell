// SPDX-License-Identifier: GPL-3.0-only
use super::placement::place_in_flow;
use super::weights::{WeightedChildren, weight};
use super::{Layout, Measuring, Visibility};
use crate::geometry::{Axis, Constraints, Point, Size};
use crate::ui::{Arrangement, Flow, ModifierElement};
use crate::units::Dp;
use crate::view::TextMeasurer;

impl<T: TextMeasurer> Measuring<'_, T> {
    pub(super) fn measure_children(
        &mut self,
        index: usize,
        arrangement: Arrangement,
        c: Constraints,
    ) -> Size {
        let children = self.nodes[index].children.clone();
        let gap = self.nodes[index].gap;
        let vertical = arrangement == Arrangement::Along(Axis::Vertical);
        let overlay = arrangement == Arrangement::Overlay;
        let scroll_axis = self.nodes[index].scroll_axis();
        let mut sizes = vec![Size::ZERO; children.len()];
        let mut in_flow: Vec<_> = children
            .iter()
            .map(|child| matches!(self.nodes[*child].flow, Flow::In(_)))
            .collect();
        let main = if vertical { c.height } else { c.width };
        let mut loose = Constraints {
            width: c.width.loosened(),
            height: c.height.loosened(),
        };
        if let Some(axis) = scroll_axis {
            loose = axis.with_span(loose, axis.span(loose).unbounded());
        }
        let bounded = main.bound().is_some();
        let weighted = self.measure_without_shares(index, bounded, loose, &mut sizes, &mut in_flow);
        let length = |size: Size| if vertical { size.height } else { size.width };
        let flow_count = in_flow.iter().filter(|flow| **flow).count();
        let gaps: Dp = std::iter::repeat_n(gap, flow_count.saturating_sub(1)).sum();
        let shares = |p: usize| weighted.members.iter().any(|(position, ..)| *position == p);
        let fixed = (0..children.len())
            .filter(|&p| !overlay && in_flow[p] && !shares(p))
            .fold(gaps, |sum, p| sum + length(sizes[p]));
        let needed = self.measure_shares(&weighted, &mut sizes, c, fixed, vertical);
        let flowing = || {
            sizes
                .iter()
                .zip(&in_flow)
                .filter_map(|(size, flow)| flow.then_some(*size))
        };
        let stacked = match needed {
            Some(needed) if main.allows_more() => fixed + needed,
            _ => flowing().map(length).sum::<Dp>() + gaps,
        };
        let widest = flowing().map(|s| s.width).fold(Dp::ZERO, Dp::max);
        let tallest = flowing().map(|s| s.height).fold(Dp::ZERO, Dp::max);
        let content = if overlay {
            Size::new(widest, tallest)
        } else if vertical {
            Size::new(widest, stacked)
        } else {
            Size::new(stacked, tallest)
        };
        self.layout[index].content = content;
        let extent = scroll_axis.map_or(content, |axis| {
            axis.with_length(content, axis.length(content).min(axis.span(c).proposed()))
        });
        let actual = c.constrain(extent);
        let align = self.nodes[index].align;
        let placements = place_in_flow(&sizes, &in_flow, actual, align, gap, arrangement);
        self.layout[index].children = placements;
        actual
    }

    /// Resolves the conditions of the children of the node at `index` in the order of measuring, and measures under
    /// `loose` each shown child that takes no share. A hidden child leaves the flow. Returns the shown children that
    /// take a share of the length of the node.
    fn measure_without_shares(
        &mut self,
        index: usize,
        bounded: bool,
        loose: Constraints,
        sizes: &mut [Size],
        in_flow: &mut [bool],
    ) -> WeightedChildren {
        let nodes = self.nodes;
        let children = &nodes[index].children;
        let order = self.graph.order(index, children.len());
        let weights: Vec<_> = (children.iter())
            .map(|&child| weight(nodes, index, child, bounded))
            .collect();
        for &p in order.iter().filter(|&&p| weights[p].is_none()) {
            if self.resolve_conditions(children[p]) {
                sizes[p] = self.measure(children[p], 0, loose);
            } else {
                in_flow[p] = false;
            }
        }
        let mut members = Vec::new();
        for &p in &order {
            let Some(weight) = weights[p] else {
                continue;
            };
            if !self.resolve_conditions(children[p]) {
                in_flow[p] = false;
            } else if self.takes_share(index, children[p]) {
                members.push((p, children[p], weight));
            } else {
                sizes[p] = self.measure(children[p], 0, loose);
            }
        }
        let total = members.iter().map(|(.., w)| w.factor()).sum();
        WeightedChildren { members, total }
    }

    /// Returns whether every condition of the node at `index` holds, and sets the visibility of the node by it. A
    /// hidden node is also laid out as hidden.
    pub(super) fn resolve_conditions(&mut self, index: usize) -> bool {
        let nodes = self.nodes;
        let shown = (nodes[index].modifier.iter()).all(|element| match element {
            ModifierElement::ShownWhen(condition) => condition.resolve(self),
            _ => true,
        });
        if shown {
            self.layout[index].visibility = Visibility::Shown;
        } else {
            self.hide(index);
        }
        shown
    }

    /// Lays out the node at `index` and the nodes inside it as hidden. Each of these nodes has no size and sits at the
    /// origin of the content of its parent node. The natural width stays stored, because it does not depend on
    /// whether the node is shown.
    fn hide(&mut self, index: usize) {
        let nodes = self.nodes;
        self.layout[index] = Layout {
            visibility: Visibility::Hidden,
            children: vec![Point::default(); nodes[index].children.len()],
            natural_width: self.layout[index].natural_width,
            ..Layout::default()
        };
        self.filled[index] = true;
        for &child in &nodes[index].children {
            self.hide(child);
        }
    }
}
