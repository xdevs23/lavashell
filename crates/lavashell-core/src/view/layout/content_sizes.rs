// SPDX-License-Identifier: GPL-3.0-only
use super::{Constraints, MeasuredSizes, Measuring, Pass, Span, TextMeasurer};
use crate::ui::ContentSizes;
use crate::units::Dp;

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
        let open = Constraints {
            width: Span::offering(Dp::INFINITY),
            height: Span::offering(Dp::INFINITY),
        };
        let smallest = Constraints {
            width: Span::offering(Dp::ZERO),
            height: Span::offering(Dp::ZERO),
        };
        let mut probe = Measuring {
            layout: self.layout.clone(),
            filled: self.filled.clone(),
            ..Measuring::new(
                self.nodes,
                self.graph,
                Pass::Probe(MeasuredSizes(vec![Vec::new(); self.nodes.len()])),
                self.text,
            )
        };
        let preferred = probe.measure(index, 0, open);
        let minimum = probe.measure(index, 0, smallest);
        self.content_sizes[index] = Some(ContentSizes { preferred, minimum });
    }
}
