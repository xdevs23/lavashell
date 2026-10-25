// SPDX-License-Identifier: GPL-3.0-only
use super::height_index::HeightIndex;
use crate::DuplicateKey;
use lavashell_core::prelude::*;
use std::{collections::HashMap, hash::Hash};

/// The keys of the rows of a measured list in the order of the rows, with the row index of each key.
/// Each key belongs to one row only.
pub(super) struct RowKeys<K> {
    keys: Vec<K>,
    slots: HashMap<K, usize>,
}

impl<K: Clone + Eq + Hash> RowKeys<K> {
    /// The row keys in the order of `keys`.
    ///
    /// # Errors
    ///
    /// Fails when two keys are equal.
    pub(super) fn new(keys: Vec<K>) -> Result<Self, DuplicateKey> {
        let slots: HashMap<_, _> = keys.iter().cloned().zip(0..).collect();
        if slots.len() == keys.len() {
            Ok(Self { keys, slots })
        } else {
            Err(DuplicateKey)
        }
    }
}

/// The keys of the rows of a measured list, the row heights measured at the width of the view, and
/// the size of one device pixel of the view.
pub(super) struct MeasuredRows<K> {
    keys: Vec<K>,
    slots: HashMap<K, usize>,
    index: HeightIndex,
    width: Dp,
    pixel: Dp,
}

impl<K> MeasuredRows<K> {
    /// An empty list of rows at width zero.
    pub(super) fn new() -> Self {
        Self {
            keys: Vec::new(),
            slots: HashMap::new(),
            index: HeightIndex::new(Vec::new()),
            width: Dp::ZERO,
            pixel: Dp::ZERO,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.keys.len()
    }

    pub(super) fn key(&self, slot: usize) -> &K {
        &self.keys[slot]
    }

    pub(super) fn pixel(&self) -> Dp {
        self.pixel
    }

    pub(super) fn set_pixel(&mut self, pixel: Dp) {
        self.pixel = pixel;
    }

    /// Whether `a` and `b` differ by less than one device pixel. Layout cannot show a smaller
    /// difference.
    pub(super) fn same_length(&self, a: Dp, b: Dp) -> bool {
        (a - b).abs() < self.pixel
    }

    pub(super) fn has_width(&self, width: Dp) -> bool {
        self.same_length(self.width, width)
    }

    /// Sets the width of the view and removes every measured height, because no row is measured at
    /// the new width yet.
    pub(super) fn change_width(&mut self, width: Dp) {
        self.width = width;
        self.index.forget_measurements();
    }

    /// The height of the row at `slot`: its measured height, or the mean height of the measured rows
    /// for a row not measured yet.
    pub(super) fn height(&self, slot: usize) -> Dp {
        self.index.height(slot)
    }

    /// Sets the measured height of the row at `slot`.
    pub(super) fn measure(&mut self, slot: usize, height: Dp) {
        self.index.measure(slot, height);
    }

    /// The sum of the heights of the rows before `end`: the top of the row at `end`, or the height of
    /// all rows when `end` is the number of rows.
    pub(super) fn prefix(&self, end: usize) -> Dp {
        self.index.prefix(end)
    }

    pub(super) fn total(&self) -> Dp {
        self.index.total()
    }

    /// The row that contains `offset`, or the last row for an offset past the end. Returns the first
    /// row while no height is known, so the first row is composed and measured first.
    pub(super) fn find(&self, offset: Dp) -> usize {
        self.index.find(offset)
    }
}

impl<K: Clone + Eq + Hash> MeasuredRows<K> {
    pub(super) fn has_keys(&self, keys: &[K]) -> bool {
        self.keys == keys
    }

    /// Replaces the keys of the rows with `keys`. A key that was in the list before gets the height
    /// measured for it before.
    pub(super) fn replace_keys(&mut self, keys: RowKeys<K>) {
        let RowKeys { keys, slots } = keys;
        let previous: HashMap<_, _> = (self.keys.iter().cloned())
            .zip(self.index.heights().iter().copied())
            .collect();
        let heights = (keys.iter())
            .map(|key| previous.get(key).copied().flatten())
            .collect();
        self.index.replace(heights);
        self.keys = keys;
        self.slots = slots;
    }

    /// The row index of `key` and the height to store for that row, from the height `measured` at
    /// `width`. The height is at least one device pixel. Returns `None` for a measurement at an old
    /// width, for a removed key, and for a height already stored for the row.
    pub(super) fn accepted(&self, key: &K, width: Dp, measured: Dp) -> Option<(usize, Dp)> {
        if !self.has_width(width) {
            return None;
        }
        let slot = *self.slots.get(key)?;
        let height = measured.max(self.pixel);
        let taken = self.index.heights()[slot];
        let same = taken.is_some_and(|taken| self.same_length(taken, height));
        (!same).then_some((slot, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TestError;

    fn with_keys(keys: &[char]) -> Result<MeasuredRows<char>, DuplicateKey> {
        let mut rows = MeasuredRows::new();
        rows.set_pixel(1.dp());
        rows.change_width(300.dp());
        rows.replace_keys(RowKeys::new(keys.to_vec())?);
        Ok(rows)
    }

    #[test]
    fn reordering_the_keys_keeps_the_heights_of_the_keys_that_stay() -> Result<(), TestError> {
        let mut rows = with_keys(&['a', 'b', 'c'])?;
        rows.measure(1, 150.dp());
        rows.replace_keys(RowKeys::new(vec!['c', 'b', 'a', 'd'])?);
        assert_eq!(rows.accepted(&'b', 300.dp(), 150.dp()), None);
        assert_eq!(rows.accepted(&'b', 300.dp(), 90.dp()), Some((1, 90.dp())));
        assert_eq!(rows.accepted(&'a', 300.dp(), 90.dp()), Some((2, 90.dp())));
        Ok(())
    }

    #[test]
    fn a_measurement_is_taken_only_for_a_key_that_stays_at_the_current_width()
    -> Result<(), TestError> {
        let rows = with_keys(&['a', 'b'])?;
        assert_eq!(rows.accepted(&'a', 400.dp(), 90.dp()), None, "an old width");
        assert_eq!(
            rows.accepted(&'z', 300.dp(), 90.dp()),
            None,
            "a removed key"
        );
        assert_eq!(
            rows.accepted(&'a', 300.dp(), 0.dp()),
            Some((0, 1.dp())),
            "no row is lower than a pixel"
        );
        Ok(())
    }

    #[test]
    fn two_equal_keys_are_refused() {
        assert!(matches!(RowKeys::new(vec!['a', 'a']), Err(DuplicateKey)));
    }
}
