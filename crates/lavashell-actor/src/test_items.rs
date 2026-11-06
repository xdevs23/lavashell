// SPDX-License-Identifier: GPL-3.0-only
use crate::{Effect, Merge};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    Set { key: u8, value: f64 },
    Step(f64),
    Fence(u8),
}

impl Merge for Item {
    type Key = u8;

    fn effect(&self) -> Effect<u8> {
        match *self {
            Self::Set { key, .. } => Effect::Overwrites(key),
            Self::Step(_) => Effect::BuildsOn,
            Self::Fence(key) => Effect::Fences(key),
        }
    }
}

/// An amount that adds up with the later amounts of its key.
#[derive(Debug, PartialEq)]
pub(crate) struct Sum {
    pub(crate) key: u8,
    pub(crate) amount: f64,
}

impl Merge for Sum {
    type Key = u8;

    fn effect(&self) -> Effect<u8> {
        Effect::Overwrites(self.key)
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
