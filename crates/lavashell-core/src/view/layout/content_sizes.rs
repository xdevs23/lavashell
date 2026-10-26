// SPDX-License-Identifier: GPL-3.0-only
use super::{Constraints, MeasuredSizes, Measuring, Pass, Span, TextMeasurer};
use crate::ui::ContentSizes;
use crate::units::Dp;
use crate::view::collect_descendants;
use std::collections::BTreeSet;

impl<T: TextMeasurer> Measuring<'_, T> {
    /// Measures the size of the node at `index` without bounds and its smallest size, once per frame and only for a
    /// node with an `on_content_sizes` action. A separate probe measures both sizes, starting from a copy of the
    /// layout filled in so far. The layout of the frame and the sizes reported from it are therefore the ones measured
    /// by the frame itself.
    pub(super) fn measure_content_sizes(&mut self, index: usize) {
        let pending = matches!(self.pass, Pass::Frame(_)) && self.content_sizes[index].is_none();
        if !pending || self.nodes[index].content_sizes.is_none() {
            return;
        }
        let smallest = Constraints {
            width: Span::offering(Dp::ZERO),
            height: Span::offering(Dp::ZERO),
        };
        let mut probe = self.probe();
        let preferred = probe.measure(index, 0, open());
        let minimum = probe.measure(index, 0, smallest);
        self.content_sizes[index] = Some(ContentSizes { preferred, minimum });
    }

    /// The width of the node at `index` at an unbounded proposal, before the width set on it. The node and the nodes
    /// inside it are measured in place and then restored, so the cost grows with that subtree alone. The width stays
    /// stored for the rest of the pass, and is measured again only where a read of a size passes through the node.
    pub(in crate::view) fn natural_width_at(&mut self, index: usize) -> Dp {
        if let Some(width) = self.layout[index].natural_width
            && !self.graph.is_crossed(index)
        {
            return width;
        }
        let mut subtree = BTreeSet::new();
        collect_descendants(self.nodes, index, &mut subtree);
        let saved: Vec<_> = (subtree.iter())
            .map(|&node| (node, self.layout[node].clone(), self.filled[node]))
            .collect();
        let measured = self.measured.len();
        let pass = std::mem::replace(&mut self.pass, Pass::Probe(MeasuredSizes::default()));
        let outer = self.natural_width_of.replace(index);
        let width = self.measure(index, 0, open()).width;
        self.natural_width_of = outer;
        self.pass = pass;
        self.measured.truncate(measured);
        for (node, layout, filled) in saved {
            self.layout[node] = layout;
            self.filled[node] = filled;
        }
        self.layout[index].natural_width = Some(width);
        width
    }

    /// Measures the natural widths read by offsets and positions. The placement pass reads them after the measuring
    /// pass has finished.
    pub(super) fn measure_natural_widths_read_by_placement(&mut self) {
        let graph = self.graph;
        for &index in graph.nodes_with_natural_width_read_by_placement() {
            let _read_when_placing = self.natural_width_at(index);
        }
    }

    /// Returns a probe that measures separately from this pass. The probe starts from a copy of the layout filled in
    /// so far and does not call the callbacks.
    fn probe(&mut self) -> Measuring<'_, T> {
        Measuring {
            layout: self.layout.clone(),
            filled: self.filled.clone(),
            ..Measuring::new(
                self.nodes,
                self.graph,
                Pass::Probe(MeasuredSizes::default()),
                self.text,
            )
        }
    }
}

fn open() -> Constraints {
    Constraints {
        width: Span::offering(Dp::INFINITY),
        height: Span::offering(Dp::INFINITY),
    }
}
