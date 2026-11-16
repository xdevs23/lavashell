// SPDX-License-Identifier: GPL-3.0-only
use crate::{Merge, MergeEffect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    Set { key: u8, value: f64 },
    SetFencing { key: u8, fenced: u8, value: f64 },
    Step(f64),
    Fence(u8),
}

impl Merge for Item {
    type Key = u8;

    fn effect(&self) -> MergeEffect<u8> {
        match *self {
            Self::Set { key, .. } => MergeEffect::Overwrites(key),
            Self::SetFencing { key, fenced, .. } => {
                MergeEffect::OverwritesAndFences { key, fenced }
            }
            Self::Step(_) => MergeEffect::Appends,
            Self::Fence(key) => MergeEffect::Fences(key),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Sum {
    pub(crate) key: u8,
    pub(crate) amount: f64,
}

impl Merge for Sum {
    type Key = u8;

    fn effect(&self) -> MergeEffect<u8> {
        MergeEffect::Overwrites(self.key)
    }

    fn absorb(&mut self, later: Self) {
        self.amount += later.amount;
    }
}

pub(crate) fn set(key: u8, value: f64) -> Item {
    Item::Set { key, value }
}

pub(crate) fn step(number: f64) -> Item {
    Item::Step(number)
}
