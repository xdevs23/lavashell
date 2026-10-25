// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::*;
use std::ops::AddAssign;

/// The sum over a range of rows: the sum of their measured heights, the number of measured rows and
/// the number of rows that are not measured. The counts change by one at a time, so they are exact
/// as `f64` and need no conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RowSum {
    height: f64,
    measured: f64,
    unmeasured: f64,
}

impl RowSum {
    /// The sum over one row with the measured height `height`, or over one row that is not measured
    /// for `None`.
    fn of(height: Option<Dp>) -> Self {
        match height {
            Some(height) => Self {
                height: height.get(),
                measured: 1.0,
                unmeasured: 0.0,
            },
            None => Self {
                height: 0.0,
                measured: 0.0,
                unmeasured: 1.0,
            },
        }
    }
}

impl AddAssign for RowSum {
    fn add_assign(&mut self, other: Self) {
        self.height += other.height;
        self.measured += other.measured;
        self.unmeasured += other.unmeasured;
    }
}

/// The heights of the rows of a measured list, with a Fenwick tree for the sum of the heights above
/// a row. A row that is not measured counts as high as the mean height of the measured rows.
pub(super) struct HeightIndex {
    /// The height of each row measured at the current width, or `None` for a row not measured yet.
    heights: Vec<Option<Dp>>,
    /// A Fenwick tree over `heights`: each node contains the sum over a range of rows.
    tree: Vec<RowSum>,
    total: RowSum,
    /// The mean height of the rows measured before the width changed. It is used as the mean until
    /// a row is measured at the new width.
    carried_mean: Option<f64>,
}

impl HeightIndex {
    /// An index over rows with the heights `heights`, where `None` marks a row not measured yet.
    pub(super) fn new(heights: Vec<Option<Dp>>) -> Self {
        let mut index = Self {
            heights,
            tree: Vec::new(),
            total: RowSum::default(),
            carried_mean: None,
        };
        index.rebuild();
        index
    }

    /// Replaces the rows with rows of `heights`. `carried_mean` is unchanged.
    pub(super) fn replace(&mut self, heights: Vec<Option<Dp>>) {
        self.heights = heights;
        self.rebuild();
    }

    pub(super) fn heights(&self) -> &[Option<Dp>] {
        &self.heights
    }

    /// The mean height of the rows measured at the current width, or before the width changed.
    /// None until a row is measured.
    pub(super) fn mean(&self) -> Option<f64> {
        if self.total.measured > 0.0 {
            Some(self.total.height / self.total.measured)
        } else {
            self.carried_mean
        }
    }

    /// The height of the row at `slot`: its measured height, or the value of `mean` for a row not
    /// measured yet. Zero while `mean` has no value.
    pub(super) fn height(&self, slot: usize) -> Dp {
        self.heights[slot].unwrap_or_else(|| Dp::new(self.mean().unwrap_or_default()))
    }

    /// Sets the measured height of the row at `slot` and updates the sums that contain the row.
    pub(super) fn measure(&mut self, slot: usize, height: Dp) {
        let added = match self.heights[slot].replace(height) {
            Some(before) => RowSum {
                height: height.get() - before.get(),
                ..RowSum::default()
            },
            None => RowSum {
                height: height.get(),
                measured: 1.0,
                unmeasured: -1.0,
            },
        };
        self.add(slot, added);
        self.total += added;
    }

    /// Removes every measured height. The mean of those heights is stored in `carried_mean` and used
    /// for the rows not measured yet.
    pub(super) fn forget_measurements(&mut self) {
        self.carried_mean = self.mean();
        self.heights.fill(None);
        self.rebuild();
    }

    /// Builds the tree and `total` from `heights` in one pass: each node adds its sum to the parent
    /// node that covers its range.
    fn rebuild(&mut self) {
        self.tree = vec![RowSum::default(); self.heights.len() + 1];
        self.total = RowSum::default();
        for (slot, height) in self.heights.iter().enumerate() {
            let row = RowSum::of(*height);
            self.tree[slot + 1] += row;
            self.total += row;
        }
        for node in 1..self.tree.len() {
            let parent = node + node.isolate_lowest_one();
            if parent < self.tree.len() {
                let covered = self.tree[node];
                self.tree[parent] += covered;
            }
        }
    }

    /// Adds `added` to the sum of each node that covers `slot`.
    fn add(&mut self, slot: usize, added: RowSum) {
        let mut i = slot + 1;
        while i < self.tree.len() {
            self.tree[i] += added;
            i += i.isolate_lowest_one();
        }
    }

    /// The sum of the heights of the rows before `end`: the top of the row at `end`, or the height of
    /// all rows when `end` is the number of rows.
    pub(super) fn prefix(&self, mut end: usize) -> Dp {
        let mut sum = RowSum::default();
        while end > 0 {
            sum += self.tree[end];
            end &= end - 1;
        }
        Dp::new(sum.height + self.mean().unwrap_or_default() * sum.unmeasured)
    }

    pub(super) fn total(&self) -> Dp {
        self.prefix(self.heights.len())
    }

    /// The row that contains `offset`, or the last row for an offset past the end. Returns the first
    /// row while `mean` has no value, so the first row is composed and measured first.
    pub(super) fn find(&self, offset: Dp) -> usize {
        let offset = offset.get();
        let Some(mean) = self.mean().filter(|_| !self.heights.is_empty()) else {
            return 0;
        };
        let mut slot = 0;
        let mut sum = 0.0;
        let mut bit = 1usize << (usize::BITS - 1 - self.heights.len().leading_zeros());
        while bit > 0 {
            let next = slot + bit;
            if next < self.tree.len() {
                let node = self.tree[next];
                let height = node.height + mean * node.unmeasured;
                if sum + height <= offset {
                    slot = next;
                    sum += height;
                }
            }
            bit >>= 1;
        }
        slot.min(self.heights.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_found_by_the_offset_in_it_while_only_some_rows_are_measured() {
        let heights = [24.dp(), 36.dp(), 48.dp(), 60.dp(), 72.dp()];
        let mut index = HeightIndex::new(vec![None; 10_000]);
        for slot in (0..10_000).step_by(37) {
            index.measure(slot, heights[slot % heights.len()]);
        }
        assert_eq!(index.height(0), 24.dp());
        for offset in [0.dp(), 24.dp(), 1000.dp(), index.total() - 1.dp()] {
            let slot = index.find(offset);
            assert!(index.prefix(slot) <= offset);
            assert!(index.prefix(slot + 1) > offset);
        }
    }

    #[test]
    fn a_row_that_is_not_measured_is_as_high_as_the_mean_of_the_measured_rows() {
        let mut index = HeightIndex::new(vec![None; 10]);
        for (slot, height) in [30.dp(), 50.dp(), 70.dp()].into_iter().enumerate() {
            index.measure(slot, height);
        }
        assert_eq!(index.mean(), Some(50.0));
        assert_eq!(index.height(5), 50.dp());
        assert_eq!(index.prefix(5), 250.dp());
        assert_eq!(index.total(), 500.dp());
        index.measure(1, 80.dp());
        assert_eq!(index.mean(), Some(60.0));
        assert_eq!(index.total(), 600.dp());
    }

    #[test]
    fn the_mean_stays_for_the_rows_until_one_is_measured_again() {
        let mut index = HeightIndex::new(vec![None; 10]);
        for slot in 0..10 {
            index.measure(slot, 40.dp());
        }
        index.forget_measurements();
        assert!(index.heights().iter().all(Option::is_none));
        assert_eq!(index.mean(), Some(40.0));
        assert_eq!(index.total(), 400.dp());
        index.measure(0, 100.dp());
        assert_eq!(index.mean(), Some(100.0));
        assert_eq!(index.total(), 1000.dp());
    }
}
