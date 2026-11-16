// SPDX-License-Identifier: GPL-3.0-only
use crate::{Merge, MergeEffect};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::mem;

#[derive(Clone, Copy)]
pub(crate) enum Fence {
    NamedKey,
    EveryKey,
}

pub(crate) struct Pending<T: Merge> {
    fence: Fence,
    items: Vec<T>,
    index: HashMap<T::Key, usize>,
    appended_items: usize,
}

impl<T: Merge> Pending<T> {
    pub(crate) fn new(fence: Fence) -> Self {
        Self {
            fence,
            items: Vec::new(),
            index: HashMap::new(),
            appended_items: 0,
        }
    }

    pub(crate) fn push(&mut self, item: T) {
        match item.effect() {
            MergeEffect::Overwrites(key) => self.overwrite(key, item),
            MergeEffect::OverwritesAndFences { key, fenced } => {
                self.fence(&fenced);
                self.overwrite(key, item);
            }
            MergeEffect::Appends => self.append_fence(None, item),
            MergeEffect::Fences(key) => self.append_fence(Some(key), item),
        }
    }

    pub(crate) fn take(&mut self) -> Vec<T> {
        self.clear_index();
        self.appended_items = 0;
        mem::take(&mut self.items)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn appended_items(&self) -> usize {
        self.appended_items
    }

    fn overwrite(&mut self, key: T::Key, item: T) {
        match self.index.entry(key) {
            Entry::Occupied(place) => self.items[*place.get()].absorb(item),
            Entry::Vacant(place) => {
                let _inserted_position_is_not_needed = place.insert(self.items.len());
                self.items.push(item);
            }
        }
    }

    fn append_fence(&mut self, key: Option<T::Key>, item: T) {
        self.appended_items += 1;
        match (self.fence, key) {
            (Fence::EveryKey, _) => self.clear_index(),
            (Fence::NamedKey, Some(key)) => self.fence(&key),
            (Fence::NamedKey, None) => {}
        }
        self.items.push(item);
    }

    fn fence(&mut self, key: &T::Key) {
        let _removed_position_is_not_needed = self.index.remove(key);
    }

    fn clear_index(&mut self) {
        if !self.index.is_empty() {
            self.index.clear();
        }
    }
}
