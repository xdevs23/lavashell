// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::*;
use lavashell_core::view::CompositionError;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HolderId(u64);

impl HolderId {
    fn following(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Generation(u64);

impl Generation {
    fn following(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ViewTypeId(pub u64);

#[derive(Clone, Debug)]
pub struct RecycledItem<K> {
    pub index: usize,
    pub key: K,
    pub holder: HolderId,
    pub generation: Generation,
    pub size: Size,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RecyclerStats {
    pub items: usize,
    pub active: usize,
    pub holders: usize,
    pub created: u64,
    pub rebound: u64,
    pub retained: u64,
}

struct Holder<K> {
    id: HolderId,
    kind: ViewTypeId,
    key: Option<K>,
    generation: Generation,
}

struct PoolState<K> {
    holders: Vec<Holder<K>>,
    stats: RecyclerStats,
    next: HolderId,
}

pub(crate) struct HolderPool<K>(Rc<RefCell<PoolState<K>>>);

impl<K> Clone for HolderPool<K> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K> Default for HolderPool<K> {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(PoolState {
            holders: Vec::new(),
            stats: RecyclerStats::default(),
            next: HolderId::default(),
        })))
    }
}

impl<K> HolderPool<K> {
    pub fn stats(&self) -> RecyclerStats {
        self.0.borrow().stats
    }
}

impl<K: Clone + Eq + Hash> PoolState<K> {
    fn release_unwanted(&mut self, wanted: &HashSet<(K, ViewTypeId)>) -> usize {
        let mut released = 0usize;
        for holder in &mut self.holders {
            if holder
                .key
                .as_ref()
                .is_some_and(|k| !wanted.contains(&(k.clone(), holder.kind)))
            {
                holder.key = None;
                released += 1;
            }
        }
        released
    }

    fn slots_by_binding(&self) -> HashMap<(K, ViewTypeId), usize> {
        self.holders
            .iter()
            .enumerate()
            .filter_map(|(slot, h)| h.key.as_ref().map(|k| ((k.clone(), h.kind), slot)))
            .collect()
    }

    fn slot_for(
        &mut self,
        retained: &HashMap<(K, ViewTypeId), usize>,
        key: &K,
        kind: ViewTypeId,
    ) -> usize {
        if let Some(slot) = retained.get(&(key.clone(), kind)) {
            self.stats.retained += 1;
            *slot
        } else if let Some(slot) = self
            .holders
            .iter()
            .position(|h| h.key.is_none() && h.kind == kind)
        {
            self.rebind(slot, key)
        } else {
            self.create(key, kind)
        }
    }

    fn rebind(&mut self, slot: usize, key: &K) -> usize {
        self.stats.rebound += 1;
        let holder = &mut self.holders[slot];
        holder.generation = holder.generation.following();
        holder.key = Some(key.clone());
        slot
    }

    fn create(&mut self, key: &K, kind: ViewTypeId) -> usize {
        let id = self.next;
        self.next = id.following();
        self.stats.created += 1;
        self.holders.push(Holder {
            id,
            kind,
            key: Some(key.clone()),
            generation: Generation::default(),
        });
        self.holders.len() - 1
    }

    fn drop_unbound_beyond(&mut self, mut spare: usize) {
        self.holders.retain(|h| {
            h.key.is_some() || {
                let keep = spare > 0;
                spare = spare.saturating_sub(1);
                keep
            }
        });
    }
}

impl<K: Clone + Eq + Hash> HolderPool<K> {
    pub fn bind(
        &self,
        count: usize,
        requested: Vec<(usize, K, ViewTypeId)>,
        size: impl Fn(usize) -> Size,
    ) -> Result<Vec<RecycledItem<K>>, CompositionError> {
        let identities: HashSet<_> = requested.iter().map(|(_, key, _)| key).collect();
        if identities.len() != requested.len() {
            return Err(CompositionError::DuplicateItemKey);
        }
        let wanted: HashSet<_> = requested
            .iter()
            .map(|(_, k, kind)| (k.clone(), *kind))
            .collect();
        let mut pool = self.0.borrow_mut();
        let released = pool.release_unwanted(&wanted);
        let retained = pool.slots_by_binding();
        let mut bindings = Vec::with_capacity(requested.len());
        for (index, key, kind) in requested {
            let slot = pool.slot_for(&retained, &key, kind);
            let holder = &pool.holders[slot];
            bindings.push(RecycledItem {
                index,
                key,
                holder: holder.id,
                generation: holder.generation,
                size: size(index),
            });
        }
        pool.drop_unbound_beyond(released);
        pool.stats.items = count;
        pool.stats.active = bindings.len();
        pool.stats.holders = pool.holders.len();
        Ok(bindings)
    }
}
