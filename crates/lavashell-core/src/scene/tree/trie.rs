// SPDX-License-Identifier: GPL-3.0-only
//! A persistent map from `u64` keys to values. A clone shares every branch and value with the original map. A write
//! copies only the shared branches on the path to the changed value, and that value.

use std::sync::Arc;

/// The number of key bits that pick the slot of a child in a branch, so that the slots of a branch fill one bitmap
/// word.
const STEP: u32 = u64::BITS.trailing_zeros();

#[derive(Clone)]
pub(super) struct Trie<V> {
    root: Arc<Branch<V>>,
}

#[derive(Clone)]
struct Branch<V> {
    /// Bit `n` is set when the branch has a child in slot `n`. The children are in slot order.
    occupied: u64,
    children: Vec<Child<V>>,
}

#[derive(Clone)]
enum Child<V> {
    Leaf(u64, Arc<V>),
    Branch(Arc<Branch<V>>),
}

/// The lowest of the key bits that pick the slots of a branch. Two different keys that reach the deepest branch differ
/// in the key bits of that branch.
#[derive(Clone, Copy)]
struct Depth(u32);

/// The slot of a key in a branch, as the single bit of that slot in the bitmap of the branch.
#[derive(Clone, Copy)]
struct Slot(u64);

impl Depth {
    const ROOT: Self = Self(0);
    const DEEPEST: Self = Self((u64::BITS - 1) / STEP * STEP);

    fn next(self) -> Self {
        Self((self.0 + STEP).min(Self::DEEPEST.0))
    }
}

impl Slot {
    fn of(key: u64, depth: Depth) -> Self {
        Self(1_u64 << ((key >> depth.0) % u64::from(u64::BITS)))
    }
}

impl<V> Default for Trie<V> {
    fn default() -> Self {
        Self {
            root: Arc::new(Branch::default()),
        }
    }
}

impl<V> Default for Branch<V> {
    fn default() -> Self {
        Self {
            occupied: 0,
            children: Vec::new(),
        }
    }
}

impl<V> Trie<V> {
    pub(super) fn get(&self, key: u64) -> Option<&V> {
        self.root.get(key, Depth::ROOT)
    }

    fn contains_key(&self, key: u64) -> bool {
        self.get(key).is_some()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.root.children.is_empty()
    }
}

impl<V: Clone> Trie<V> {
    /// Copies the shared branches on the path to the value of `key`, and the value, only when `key` is present.
    pub(super) fn get_mut(&mut self, key: u64) -> Option<&mut V> {
        if !self.contains_key(key) {
            return None;
        }
        Arc::make_mut(&mut self.root).get_mut(key, Depth::ROOT)
    }

    /// Puts `value` under `key`, in place of the value that `key` had.
    pub(super) fn insert(&mut self, key: u64, value: V) {
        Arc::make_mut(&mut self.root).insert(key, Arc::new(value), Depth::ROOT);
    }

    pub(super) fn remove(&mut self, key: u64) -> Option<Arc<V>> {
        if !self.contains_key(key) {
            return None;
        }
        Arc::make_mut(&mut self.root).remove(key, Depth::ROOT)
    }
}

impl<V> Branch<V> {
    /// The index in `children` of the child in `slot`, or the index at which that child would be inserted.
    fn position(&self, slot: Slot) -> usize {
        (self.occupied & (slot.0 - 1)).count_ones() as usize
    }

    fn index_of(&self, slot: Slot) -> Option<usize> {
        (self.occupied & slot.0 != 0).then(|| self.position(slot))
    }

    fn child(&self, slot: Slot) -> Option<&Child<V>> {
        self.children.get(self.index_of(slot)?)
    }

    fn child_mut(&mut self, slot: Slot) -> Option<&mut Child<V>> {
        let index = self.index_of(slot)?;
        self.children.get_mut(index)
    }

    fn get(&self, key: u64, depth: Depth) -> Option<&V> {
        match self.child(Slot::of(key, depth))? {
            Child::Leaf(stored, value) => (*stored == key).then_some(&**value),
            Child::Branch(branch) => branch.get(key, depth.next()),
        }
    }
}

impl<V: Clone> Branch<V> {
    fn get_mut(&mut self, key: u64, depth: Depth) -> Option<&mut V> {
        match self.child_mut(Slot::of(key, depth))? {
            Child::Leaf(stored, value) => (*stored == key).then(move || Arc::make_mut(value)),
            Child::Branch(branch) => Arc::make_mut(branch).get_mut(key, depth.next()),
        }
    }

    fn insert(&mut self, key: u64, value: Arc<V>, depth: Depth) {
        let slot = Slot::of(key, depth);
        if let Some(child) = self.child_mut(slot) {
            child.insert(key, value, depth);
        } else {
            let position = self.position(slot);
            self.children.insert(position, Child::Leaf(key, value));
            self.occupied |= slot.0;
        }
    }

    /// Removes the value of `key`. A branch left empty is removed.
    fn remove(&mut self, key: u64, depth: Depth) -> Option<Arc<V>> {
        let slot = Slot::of(key, depth);
        let index = self.index_of(slot)?;
        let removed = match self.children.get_mut(index)? {
            Child::Leaf(stored, value) if *stored == key => Arc::clone(value),
            Child::Leaf(..) => return None,
            Child::Branch(branch) => {
                let branch = Arc::make_mut(branch);
                let removed = branch.remove(key, depth.next())?;
                if !branch.children.is_empty() {
                    return Some(removed);
                }
                removed
            }
        };
        let _removed_child = self.children.remove(index);
        self.occupied &= !slot.0;
        Some(removed)
    }
}

impl<V: Clone> Child<V> {
    /// Puts `value` under `key` in the place of this child, which is the slot of `key` in a branch at `depth`.
    fn insert(&mut self, key: u64, value: Arc<V>, depth: Depth) {
        match self {
            Self::Branch(branch) => Arc::make_mut(branch).insert(key, value, depth.next()),
            Self::Leaf(stored, old) if *stored == key => *old = value,
            Self::Leaf(stored, old) => {
                let mut split = Branch::default();
                split.insert(*stored, Arc::clone(old), depth.next());
                split.insert(key, value, depth.next());
                *self = Self::Branch(Arc::new(split));
            }
        }
    }
}
