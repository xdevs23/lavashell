// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

const STEP: u32 = u64::BITS.trailing_zeros();

#[derive(Clone)]
pub(super) struct Trie<V> {
    root: Arc<Branch<V>>,
    len: usize,
}

#[derive(Clone)]
struct Branch<V> {
    occupied: u64,
    children: Vec<Child<V>>,
}

#[derive(Clone)]
enum Child<V> {
    Leaf(u64, Arc<V>),
    Branch(Arc<Branch<V>>),
}

#[derive(Clone, Copy)]
struct Depth(u32);

#[derive(Clone, Copy)]
struct ChildBit(u64);

impl Depth {
    const ROOT: Self = Self(0);
    const DEEPEST: Self = Self((u64::BITS - 1) / STEP * STEP);

    fn next(self) -> Self {
        Self((self.0 + STEP).min(Self::DEEPEST.0))
    }
}

impl ChildBit {
    fn of(key: u64, depth: Depth) -> Self {
        Self(1_u64 << ((key >> depth.0) % u64::from(u64::BITS)))
    }
}

impl<V> Default for Trie<V> {
    fn default() -> Self {
        Self {
            root: Arc::new(Branch::default()),
            len: 0,
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

    pub(super) fn len(&self) -> usize {
        self.len
    }
}

impl<V: Clone> Trie<V> {
    pub(super) fn get_mut(&mut self, key: u64) -> Option<&mut V> {
        if !self.contains_key(key) {
            return None;
        }
        Arc::make_mut(&mut self.root).get_mut(key, Depth::ROOT)
    }

    pub(super) fn insert(&mut self, key: u64, value: V) {
        self.len += usize::from(!self.contains_key(key));
        Arc::make_mut(&mut self.root).insert(key, Arc::new(value), Depth::ROOT);
    }

    pub(super) fn remove(&mut self, key: u64) -> Option<Arc<V>> {
        if !self.contains_key(key) {
            return None;
        }
        let removed = Arc::make_mut(&mut self.root).remove(key, Depth::ROOT);
        self.len -= usize::from(removed.is_some());
        removed
    }
}

impl<V> Branch<V> {
    fn position(&self, bit: ChildBit) -> usize {
        (self.occupied & (bit.0 - 1)).count_ones() as usize
    }

    fn index_of(&self, bit: ChildBit) -> Option<usize> {
        (self.occupied & bit.0 != 0).then(|| self.position(bit))
    }

    fn child(&self, bit: ChildBit) -> Option<&Child<V>> {
        self.children.get(self.index_of(bit)?)
    }

    fn child_mut(&mut self, bit: ChildBit) -> Option<&mut Child<V>> {
        let index = self.index_of(bit)?;
        self.children.get_mut(index)
    }

    fn get(&self, key: u64, depth: Depth) -> Option<&V> {
        match self.child(ChildBit::of(key, depth))? {
            Child::Leaf(stored, value) => (*stored == key).then_some(&**value),
            Child::Branch(branch) => branch.get(key, depth.next()),
        }
    }
}

impl<V: Clone> Branch<V> {
    fn get_mut(&mut self, key: u64, depth: Depth) -> Option<&mut V> {
        match self.child_mut(ChildBit::of(key, depth))? {
            Child::Leaf(stored, value) => (*stored == key).then(move || Arc::make_mut(value)),
            Child::Branch(branch) => Arc::make_mut(branch).get_mut(key, depth.next()),
        }
    }

    fn insert(&mut self, key: u64, value: Arc<V>, depth: Depth) {
        let bit = ChildBit::of(key, depth);
        if let Some(child) = self.child_mut(bit) {
            child.insert(key, value, depth);
        } else {
            let position = self.position(bit);
            self.children.insert(position, Child::Leaf(key, value));
            self.occupied |= bit.0;
        }
    }

    fn remove(&mut self, key: u64, depth: Depth) -> Option<Arc<V>> {
        let bit = ChildBit::of(key, depth);
        let index = self.index_of(bit)?;
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
        self.occupied &= !bit.0;
        Some(removed)
    }
}

impl<V: Clone> Child<V> {
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
