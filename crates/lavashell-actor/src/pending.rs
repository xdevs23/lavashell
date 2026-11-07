// SPDX-License-Identifier: GPL-3.0-only
use crate::{Effect, Merge};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::mem;

/// Which earlier writes an item that does not overwrite fences, so that no later overwrite replaces them.
#[derive(Clone, Copy)]
pub(crate) enum Fence {
    /// An item that fences a key fences the writes of that key.
    NamedKey,
    /// Every item that does not overwrite fences the writes of every key.
    EveryKey,
}

/// Items in the order in which they were pushed. An overwrite is absorbed by the earlier write with the same key,
/// unless a fence came after that write. Every other item is appended.
pub(crate) struct Pending<T: Merge> {
    fence: Fence,
    items: Vec<T>,
    /// The place of each key whose write a later overwrite replaces.
    index: HashMap<T::Key, usize>,
    builds_on: usize,
}

impl<T: Merge> Pending<T> {
    pub(crate) fn new(fence: Fence) -> Self {
        Self {
            fence,
            items: Vec::new(),
            index: HashMap::new(),
            builds_on: 0,
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        match item.effect() {
            Effect::Overwrites(key) => match self.index.entry(key) {
                Entry::Occupied(place) => {
                    self.items[*place.get()].absorb(item);
                    return;
                }
                Entry::Vacant(place) => {
                    let _place = place.insert(self.items.len());
                }
            },
            Effect::BuildsOn => self.fence_earlier_writes(None),
            Effect::Fences(key) => self.fence_earlier_writes(Some(key)),
        }
        self.items.push(item);
    }

    /// Removes and returns every item, in order.
    pub(crate) fn take(&mut self) -> Vec<T> {
        self.clear_index();
        self.builds_on = 0;
        mem::take(&mut self.items)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The number of items that do not overwrite.
    pub(crate) fn builds_on(&self) -> usize {
        self.builds_on
    }

    /// Counts an item that does not overwrite, and fences the earlier writes that the fence rule names. `key` is the
    /// key that the item fences.
    fn fence_earlier_writes(&mut self, key: Option<T::Key>) {
        self.builds_on += 1;
        match (self.fence, key) {
            (Fence::EveryKey, _) => self.clear_index(),
            (Fence::NamedKey, Some(key)) => {
                let _fenced = self.index.remove(&key);
            }
            (Fence::NamedKey, None) => {}
        }
    }

    /// Skips an empty index, because clearing a map costs time in proportion to its capacity.
    fn clear_index(&mut self) {
        if !self.index.is_empty() {
            self.index.clear();
        }
    }
}
