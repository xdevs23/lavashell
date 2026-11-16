// SPDX-License-Identifier: GPL-3.0-only
use super::Measuring;
use crate::geometry::{Constraints, Size};
use crate::units::Dp;
use crate::view::{TextMeasurer, retained};
use std::collections::BTreeMap;

pub(super) enum Pass<'a> {
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

    pub(super) fn probe_parts(&self) -> Option<MeasuredParts> {
        match self {
            Self::Frame(_) => None,
            Self::Probe(probe) => Some(probe.parts),
        }
    }
}

pub(super) struct Probe {
    sizes: MeasuredValues<Constraints, Size>,
    widths: MeasuredValues<Constraints, Dp>,
    texts: MeasuredValues<Dp, Size>,
    parts: MeasuredParts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MeasuredParts {
    Both,
    WidthOnly,
}

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
