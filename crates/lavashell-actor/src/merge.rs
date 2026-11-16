// SPDX-License-Identifier: GPL-3.0-only
use std::hash::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeEffect<K> {
    Overwrites(K),
    OverwritesAndFences { key: K, fenced: K },
    Appends,
    Fences(K),
}

impl<K> MergeEffect<K> {
    pub fn map<L>(self, convert: impl Fn(K) -> L) -> MergeEffect<L> {
        match self {
            Self::Overwrites(key) => MergeEffect::Overwrites(convert(key)),
            Self::OverwritesAndFences { key, fenced } => MergeEffect::OverwritesAndFences {
                key: convert(key),
                fenced: convert(fenced),
            },
            Self::Appends => MergeEffect::Appends,
            Self::Fences(key) => MergeEffect::Fences(convert(key)),
        }
    }
}

pub trait Merge: Sized {
    type Key: Eq + Hash;

    fn effect(&self) -> MergeEffect<Self::Key>;

    fn absorb(&mut self, later: Self) {
        *self = later;
    }

    fn passes_closed_intake(&self) -> bool {
        matches!(self.effect(), MergeEffect::Overwrites(_))
    }
}
