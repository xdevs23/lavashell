// SPDX-License-Identifier: GPL-3.0-only
use super::Measuring;
use crate::geometry::{Constraints, Span};
use crate::ui::ContentSizes;
use crate::units::Dp;
use crate::view::TextMeasurer;
use crate::view::component::collect_descendants;
use crate::view::layout::pass::Pass;
use std::collections::BTreeSet;

impl<T: TextMeasurer> Measuring<'_, T> {
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
        let pass = std::mem::replace(&mut self.pass, Pass::probe());
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

    pub(super) fn measure_natural_widths_read_by_placement(&mut self) {
        let graph = self.graph;
        for &index in graph.nodes_with_natural_width_read_by_placement() {
            let _read_when_placing = self.natural_width_at(index);
        }
    }

    fn probe(&mut self) -> Measuring<'_, T> {
        Measuring {
            layout: self.layout.clone(),
            filled: self.filled.clone(),
            ..Measuring::new(self.nodes, self.graph, Pass::probe(), self.text)
        }
    }
}

fn open() -> Constraints {
    Constraints {
        width: Span::offering(Dp::INFINITY),
        height: Span::offering(Dp::INFINITY),
    }
}
