// SPDX-License-Identifier: GPL-3.0-only
use std::hash::Hash;

/// What an item does to the items sent before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect<K> {
    /// The item sets the whole value of `K`, so it replaces the earlier item of `K` that no fence of `K` follows and
    /// that has not been handed out.
    Overwrites(K),
    /// The item depends on every item before it, so it is handed out after them.
    BuildsOn,
    /// The item depends on every item before it, and a later item that overwrites `K` is handed out after it.
    Fences(K),
}

/// An item that can merge with the earlier items that have not been handed out.
pub trait Merge: Sized {
    type Key: Eq + Hash;

    fn effect(&self) -> Effect<Self::Key>;

    /// Folds `later`, which overwrites the same key, into this item. The later item replaces this item unless the
    /// type sets another rule.
    fn absorb(&mut self, later: Self) {
        *self = later;
    }
}
