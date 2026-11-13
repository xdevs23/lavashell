// SPDX-License-Identifier: GPL-3.0-only
use std::hash::Hash;

/// What an item does to the items sent before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect<K> {
    /// The item sets the whole value of `K`, so it replaces the earlier write of `K` that has not been taken, unless a
    /// fence of `K` came after that write.
    Overwrites(K),
    /// The item overwrites `key` like `Overwrites` and fences `fenced`, so that no later write of `fenced` passes it.
    OverwritesAndFences { key: K, fenced: K },
    /// The item depends on every item before it, so it is taken after them. A mailbox treats it as a fence of every
    /// key.
    BuildsOn,
    /// The item depends on every item before it and fences `K`. A mailbox treats it as a fence of every key.
    Fences(K),
}

impl<K> Effect<K> {
    /// Returns the same effect with its key converted by `convert`.
    pub fn map<L>(self, convert: impl Fn(K) -> L) -> Effect<L> {
        match self {
            Self::Overwrites(key) => Effect::Overwrites(convert(key)),
            Self::OverwritesAndFences { key, fenced } => Effect::OverwritesAndFences {
                key: convert(key),
                fenced: convert(fenced),
            },
            Self::BuildsOn => Effect::BuildsOn,
            Self::Fences(key) => Effect::Fences(convert(key)),
        }
    }
}

/// An item that can merge with the earlier items that have not been taken. A choreographer fences a key only at
/// an item that fences that key, which holds for items whose overwrites commute with the items that build on them. A
/// mailbox fences every key at each item that does not overwrite, which holds for every item.
pub trait Merge: Sized {
    type Key: Eq + Hash;

    fn effect(&self) -> Effect<Self::Key>;

    /// Folds `later`, which overwrites the same key, into this item. The later item replaces this item unless the
    /// type sets another rule.
    fn absorb(&mut self, later: Self) {
        *self = later;
    }

    /// Whether a push of the item enters a mailbox while its intake is closed, instead of waiting until the intake
    /// opens. An overwrite that fences nothing enters by default, because while the intake is closed each key adds at
    /// most one such item.
    fn passes_closed_intake(&self) -> bool {
        matches!(self.effect(), Effect::Overwrites(_))
    }
}
