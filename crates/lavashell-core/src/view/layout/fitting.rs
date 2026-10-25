// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Constraints, Dp, Flow, Layout, Measuring, Node, Point, Size, Span, TextMeasurer, View,
};
use std::collections::BTreeMap;

impl<T: TextMeasurer> Measuring<'_, T> {
    /// Measures the children of the first-fitting container at `index` in the order of what they read: the faces in
    /// the flow at their natural size, and the children out of the flow within `c`. The first face that fits within
    /// `c`, or the last face when none fits, is laid out at the origin and recorded as the shown face.
    pub(super) fn measure_faces(&mut self, index: usize, c: Constraints) -> Size {
        let children = self.nodes[index].children.clone();
        let natural = Constraints {
            width: Span::offering(Dp::INFINITY),
            height: Span::offering(Dp::INFINITY),
        };
        let loose = Constraints {
            width: c.width.loosened(),
            height: c.height.loosened(),
        };
        let mut faces = BTreeMap::new();
        for position in self.graph.order(index, children.len()) {
            let child = children[position];
            let flows = matches!(self.nodes[child].flow, Flow::In(_));
            let size = self.measure(child, 0, if flows { natural } else { loose });
            faces.extend(flows.then_some((position, size)));
        }
        let fits =
            |size: Size| size.width <= c.width.proposed() && size.height <= c.height.proposed();
        let chosen = (faces.iter().find(|(_, size)| fits(**size)))
            .or_else(|| faces.last_key_value())
            .map(|(&position, _)| children[position]);
        let size = chosen.map_or(Size::ZERO, |chosen| self.measure(chosen, 0, loose));
        self.layout[index].shown_face = chosen.map(|chosen| self.nodes[chosen].id);
        self.layout[index].content = size;
        self.layout[index].children = vec![Point::default(); children.len()];
        c.constrain(size)
    }
}

impl Layout {
    /// Whether the node shows `child`: every child, except the faces of a first-fitting container other than the face
    /// it shows.
    pub(in crate::view) fn shows(&self, child: &Node) -> bool {
        matches!(child.flow, Flow::Out) || self.shown_face.is_none_or(|shown| shown == child.id)
    }
}

impl View {
    /// Whether the node at `index` shows its child at `position`.
    pub(in crate::view) fn shows(&self, index: usize, position: usize) -> bool {
        let child = &self.nodes[self.nodes[index].children[position]];
        self.layout
            .get(index)
            .is_none_or(|layout| layout.shows(child))
    }

    /// The indices of the children that the node at `index` shows.
    pub(in crate::view) fn shown_children(&self, index: usize) -> impl Iterator<Item = usize> {
        let layout = self.layout.get(index);
        (self.nodes[index].children.iter().copied())
            .filter(move |&child| layout.is_none_or(|layout| layout.shows(&self.nodes[child])))
    }
}
