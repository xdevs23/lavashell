// SPDX-License-Identifier: GPL-3.0-only
use super::frame_error::NodeName;
use super::graph::Graph;
use super::layout::{Layout, Visibility};
use super::parents::Parents;
use super::{View, scroll};
use crate::geometry::{Point, Rect, Size};
use crate::reactive::{Phase, Scope};
use crate::ui::{LayoutValue, Measured, ModifierElement, Node, NodeId, NodeRef, Placed};
use crate::units::Dp;
use std::collections::BTreeMap;

impl View {
    /// Places every shown node after the measuring pass. Resolves the offsets and positions of each node after the
    /// bounds read by them, and records where the layers of the node start.
    pub(super) fn place(&mut self) {
        let count = self.nodes.len();
        let mut placing = Placing {
            nodes: &self.nodes,
            parents: &self.parents,
            scopes: component_scopes(&self.nodes, &self.scope),
            graph: &self.graph,
            scrolls: &self.scrolls,
            layout: &mut self.layout,
            origins: vec![None; count],
            resolved: vec![false; count],
        };
        for index in 0..count {
            if placing.layout[index].visibility == Visibility::Shown {
                placing.resolve(index);
            }
        }
    }
}

struct Placing<'a> {
    nodes: &'a [Node],
    parents: &'a Parents,
    /// For each node, the scope of the nearest component, which lays out again when a value read by the placement of
    /// the node changes.
    scopes: Vec<&'a Scope>,
    graph: &'a Graph,
    scrolls: &'a BTreeMap<NodeId, Dp>,
    layout: &'a mut [Layout],
    /// Where the parent places each node found so far, in the coordinates of the view.
    origins: Vec<Option<Point>>,
    resolved: Vec<bool>,
}

/// For each node, the scope of the node itself when it is a component, or else of the nearest enclosing component,
/// found from the root down. A node outside every component gets `root`.
fn component_scopes<'a>(nodes: &'a [Node], root: &'a Scope) -> Vec<&'a Scope> {
    let mut scopes = vec![root; nodes.len()];
    let mut pending: Vec<_> = nodes.first().map(|_| (0, root)).into_iter().collect();
    while let Some((index, outer)) = pending.pop() {
        let scope = (nodes[index].component.as_ref()).map_or(outer, |component| &component.scope);
        scopes[index] = scope;
        pending.extend(nodes[index].children.iter().map(|&child| (child, scope)));
    }
    scopes
}

impl Placing<'_> {
    fn resolve(&mut self, index: usize) {
        if self.resolved[index] {
            return;
        }
        let scope = self.scopes[index];
        scope.track_further(Phase::Layout, || self.record_starts(index));
        self.resolved[index] = true;
    }

    fn record_starts(&mut self, index: usize) {
        let nodes = self.nodes;
        let mut at = Point::default();
        let mut moved = Point::default();
        let mut starts = Vec::with_capacity(nodes[index].modifier.len() + 1);
        for element in &nodes[index].modifier {
            starts.push(at);
            let by = match element.layout_value() {
                Some(LayoutValue::Offset(by)) => by.resolve(self),
                Some(LayoutValue::Position(to)) => {
                    let to = to.resolve(self);
                    let from = self.origin(index).offset(moved.x, moved.y);
                    Point::new(to.x - from.x, to.y - from.y)
                }
                _ => {
                    if let ModifierElement::Padding(insets) = element {
                        at = at.offset(insets.left, insets.top);
                    }
                    continue;
                }
            };
            at = at.offset(by.x, by.y);
            moved = moved.offset(by.x, by.y);
        }
        starts.push(at);
        self.layout[index].starts = starts;
        self.layout[index].offset = moved;
    }

    /// Where the node at `index` lies in the view with its offsets and its positions.
    fn placed(&mut self, index: usize) -> Point {
        let origin = self.origin(index);
        self.resolve(index);
        let offset = self.layout[index].offset;
        origin.offset(offset.x, offset.y)
    }

    fn origin(&mut self, index: usize) -> Point {
        if let Some(origin) = self.origins[index] {
            return origin;
        }
        let origin = match self.parents.of(index) {
            None => Point::default(),
            Some(parent) => {
                let content = self.content(parent.node);
                let at = self.layout[parent.node].children[parent.position];
                content.offset(at.x, at.y)
            }
        };
        self.origins[index] = Some(origin);
        origin
    }

    /// Where the content of the node at `index` starts in the view, after its scroll.
    fn content(&mut self, index: usize) -> Point {
        let origin = self.origin(index);
        self.resolve(index);
        let (node, layout) = (&self.nodes[index], &self.layout[index]);
        let start = layout.content_start();
        let shift = (scroll::content_clamp(node, layout)).map_or(Point::default(), |clamp| {
            let offset = || scroll::unclamped_offset(node, self.scrolls);
            clamp.translation(self.scopes[index].track_further(Phase::Layout, offset))
        });
        origin.offset(start.x + shift.x, start.y + shift.y)
    }
}

impl Measured for Placing<'_> {
    fn size(&mut self, node: &NodeRef) -> Size {
        self.layout[self.graph.node(node)].size
    }

    fn natural_width(&mut self, node: &NodeRef) -> Dp {
        let index = self.graph.node(node);
        debug_assert!(
            self.layout[index].natural_width.is_some(),
            "the natural width of {} is read before it is measured",
            NodeName::of(&self.nodes[index])
        );
        self.layout[index].natural_width.unwrap_or_default()
    }
}

impl Placed for Placing<'_> {
    fn bounds(&mut self, node: &NodeRef) -> Rect {
        let index = self.graph.node(node);
        let at = self.placed(index);
        let size = self.layout[index].size;
        Rect::new(at.x, at.y, size.width, size.height)
    }
}
