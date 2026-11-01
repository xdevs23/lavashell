// SPDX-License-Identifier: GPL-3.0-only
use super::{Constraints, Dp, Measuring, Size, TextMeasurer};
use crate::view::retained;
use std::collections::BTreeMap;

pub(super) enum Pass<'a> {
    /// A measurement for a frame, with the layouts cached for components by earlier frames. The frame reuses a cached
    /// layout where it is still valid. Otherwise the frame measures the component again inside the scope of the
    /// component, which records the values read by the layout of the component.
    Frame(&'a retained::Retained),
    Probe(Probe),
}

impl Pass<'_> {
    pub(super) fn probe() -> Self {
        Self::Probe(Probe {
            sizes: MeasuredValues::default(),
            widths: MeasuredValues::default(),
            texts: MeasuredValues::default(),
            parts: MeasuredParts::Both,
        })
    }

    pub(super) fn measured(&self, index: usize, c: Constraints) -> Option<Size> {
        match self {
            Self::Frame(_) => None,
            Self::Probe(probe) => probe.sizes.get(index, c),
        }
    }

    pub(super) fn record(&mut self, index: usize, c: Constraints, size: Size) {
        if let Self::Probe(probe) = self
            && probe.parts == MeasuredParts::Both
        {
            probe.sizes.insert(index, c, size);
        }
    }

    /// The parts of the nodes measured by a probe, or `None` for a frame, which measures every node in full.
    pub(super) fn probe_parts(&self) -> Option<MeasuredParts> {
        match self {
            Self::Frame(_) => None,
            Self::Probe(probe) => Some(probe.parts),
        }
    }
}

/// A measurement that returns a size and stores no layout.
pub(super) struct Probe {
    /// The sizes measured in full.
    sizes: MeasuredValues<Constraints, Size>,
    /// The widths measured alone.
    widths: MeasuredValues<Constraints, Dp>,
    /// The sizes of the texts, by the width that each was measured at.
    texts: MeasuredValues<Dp, Size>,
    parts: MeasuredParts,
}

/// The parts of the nodes that a probe measures.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MeasuredParts {
    Both,
    /// The widths alone. A row uses the share handed to a child that takes the whole share as the width of that child,
    /// without measuring it. The heights and the layouts found are therefore not valid.
    WidthOnly,
}

/// The values measured for each node during one probe, with the key they were measured at.
struct MeasuredValues<K, V>(BTreeMap<usize, Vec<(K, V)>>);

impl<K, V> Default for MeasuredValues<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Copy + PartialEq, V: Copy> MeasuredValues<K, V> {
    fn get(&self, index: usize, key: K) -> Option<V> {
        (self.0.get(&index)?.iter())
            .find_map(|(measured, value)| (*measured == key).then_some(*value))
    }

    fn insert(&mut self, index: usize, key: K, value: V) {
        self.0.entry(index).or_default().push((key, value));
    }
}

impl<T: TextMeasurer> Measuring<'_, T> {
    /// The width of the node at `index` under `c`, measured for the width alone, or `None` in a frame, which measures
    /// every node in full. A probe measures it once, unless a read of a size passes through the node. The width found
    /// is valid only for a node whose width does not depend on its height.
    pub(super) fn measure_width_alone(&mut self, index: usize, c: Constraints) -> Option<Dp> {
        let reusable = !self.graph.is_crossed(index);
        let Pass::Probe(probe) = &mut self.pass else {
            return None;
        };
        if reusable && let Some(width) = probe.widths.get(index, c) {
            return Some(width);
        }
        let parts = std::mem::replace(&mut probe.parts, MeasuredParts::WidthOnly);
        let width = self.measure(index, 0, c).width;
        if let Pass::Probe(probe) = &mut self.pass {
            probe.parts = parts;
            if reusable {
                probe.widths.insert(index, c, width);
            }
        }
        Some(width)
    }

    /// Measures the text of the node at `index` at `width` through `measure`. A probe measures each text once per
    /// width, because the size of a text depends on nothing else.
    pub(super) fn measure_text(
        &mut self,
        index: usize,
        width: Dp,
        measure: impl FnOnce(&mut T) -> Size,
    ) -> Size {
        if let Pass::Probe(probe) = &self.pass
            && let Some(size) = probe.texts.get(index, width)
        {
            return size;
        }
        let size = measure(self.text);
        if let Pass::Probe(probe) = &mut self.pass {
            probe.texts.insert(index, width, size);
        }
        size
    }
}
