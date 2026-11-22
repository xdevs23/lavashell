// SPDX-License-Identifier: GPL-3.0-only
use super::pass::Pass;
use super::placement::{flow_length, place_in_flow};
use super::weights::{WeightedChildren, weight};
use super::{Layout, Measuring, Visibility};
use crate::geometry::{Axis, Constraints, Point, Size};
use crate::ui::{Arrangement, Flow, ModifierElement, Overflow};
use crate::units::Dp;
use crate::view::TextMeasurer;

#[derive(Clone, Copy)]
struct FlowBound {
    axis: Axis,
    gap: Dp,
    length: Dp,
    overflow: Overflow,
}

impl<T: TextMeasurer> Measuring<'_, T> {
    pub(super) fn measure_children(
        &mut self,
        index: usize,
        arrangement: Arrangement,
        c: Constraints,
    ) -> Size {
        let children = self.nodes[index].children.clone();
        let gap = self.nodes[index].gap;
        let overlay = arrangement == Arrangement::Overlay;
        let (main_axis, overflow) = match arrangement {
            Arrangement::Along { axis, overflow } => (axis, overflow),
            Arrangement::Overlay => (Axis::Horizontal, Overflow::Visible),
        };
        let scroll_axis = self.nodes[index].scroll_axis();
        let mut sizes = vec![Size::ZERO; children.len()];
        let mut in_flow: Vec<_> = children
            .iter()
            .map(|child| matches!(self.nodes[*child].flow, Flow::In(_)))
            .collect();
        let main = main_axis.span(c);
        let mut loose = Constraints {
            width: c.width.loosened(),
            height: c.height.loosened(),
        };
        if let Some(axis) = scroll_axis {
            loose = axis.with_span(loose, axis.span(loose).unbounded());
        }
        let bound = (main.bound()).map(|length| FlowBound {
            axis: main_axis,
            gap,
            length,
            overflow,
        });
        let weighted = self.measure_without_shares(index, bound, loose, &mut sizes, &mut in_flow);
        let fixed = flow_length(&sizes, &in_flow, main_axis, gap);
        let needed = self.measure_shares(&weighted, &mut sizes, c, fixed, main_axis);
        let flowing = || {
            sizes
                .iter()
                .zip(&in_flow)
                .filter_map(|(size, flow)| flow.then_some(*size))
        };
        let stacked = match needed {
            Some(needed) if main.allows_more() => fixed + needed,
            _ => flow_length(&sizes, &in_flow, main_axis, gap),
        };
        let widest = flowing().map(|s| s.width).fold(Dp::ZERO, Dp::max);
        let tallest = flowing().map(|s| s.height).fold(Dp::ZERO, Dp::max);
        let content = if overlay {
            Size::new(widest, tallest)
        } else {
            main_axis.with_length(Size::new(widest, tallest), stacked)
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

    fn measure_without_shares(
        &mut self,
        index: usize,
        bound: Option<FlowBound>,
        loose: Constraints,
        sizes: &mut [Size],
        in_flow: &mut [bool],
    ) -> WeightedChildren {
        let nodes = self.nodes;
        let children = &nodes[index].children;
        let order = self.graph.order(index, children.len());
        let weights: Vec<_> = (children.iter())
            .map(|&child| weight(nodes, index, child, bound.is_some()))
            .collect();
        let hides = bound.is_some_and(|bound| bound.overflow != Overflow::Visible);
        let (fitted, fixed): (Vec<_>, Vec<_>) = (order.iter().copied())
            .filter(|&p| weights[p].is_none())
            .partition(|&p| hides && in_flow[p]);
        for p in fixed {
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
        if let Some(bound) = bound {
            self.keep_fitting(index, bound, fitted, loose, sizes, in_flow);
        }
        let total = members.iter().map(|(.., w)| w.factor()).sum();
        WeightedChildren { members, total }
    }

    fn keep_fitting(
        &mut self,
        index: usize,
        bound: FlowBound,
        fitted: Vec<usize>,
        loose: Constraints,
        sizes: &mut [Size],
        in_flow: &mut [bool],
    ) {
        let nodes = self.nodes;
        let children = &nodes[index].children;
        for &p in &fitted {
            in_flow[p] = false;
        }
        let mut fits = true;
        for p in bound.overflow.kept_first(fitted) {
            let child = children[p];
            if !fits {
                self.hide(child);
            } else if self.resolve_conditions(child) {
                let (measured, components) = (self.measured.len(), self.components.len());
                sizes[p] = self.measure(child, 0, loose);
                in_flow[p] = true;
                fits = flow_length(sizes, in_flow, bound.axis, bound.gap) <= bound.length;
                if !fits {
                    in_flow[p] = false;
                    self.hide_overflowing(child, loose, (measured, components));
                }
            }
        }
    }

    fn hide_overflowing(
        &mut self,
        child: usize,
        c: Constraints,
        (measured, components): (usize, usize),
    ) {
        self.components.truncate(components);
        let pass = std::mem::replace(&mut self.pass, Pass::probe());
        let _size_followed_by_the_container = self.measure(child, 0, c);
        self.pass = pass;
        self.measured.truncate(measured);
        self.hide(child);
    }

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
