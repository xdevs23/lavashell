// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::prelude::*;
use lavashell_core::view::CompositionError;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

#[derive(Clone, Debug)]
pub struct RecycledItem<K> {
    pub index: usize,
    pub key: K,
    pub holder: u64,
    pub generation: u64,
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
    id: u64,
    kind: u64,
    key: Option<K>,
    generation: u64,
}

struct PoolState<K> {
    holders: Vec<Holder<K>>,
    stats: RecyclerStats,
    next: u64,
}

/// The holders that a recycled list binds to the keys of the items in its window. A holder is a numbered slot for the
/// view of one item of one view type. A holder whose key leaves the window is bound to a new key of the same view type.
/// Its generation then goes up by one.
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
            next: 0,
        })))
    }
}

impl<K> HolderPool<K> {
    pub fn stats(&self) -> RecyclerStats {
        self.0.borrow().stats
    }
}

impl<K: Clone + Eq + Hash> HolderPool<K> {
    /// Binds a holder to each requested `(index, key, view type)` and returns the bindings in the
    /// order of `requested`. A key bound in the previous call is bound to the same holder again. A
    /// new key gets a free holder of its view type, or a new holder when none is free. Afterwards,
    /// the pool removes free holders until as many remain as this call freed, because scrolling at
    /// the same speed binds that many holders to new keys in the next call.
    ///
    /// # Errors
    ///
    /// Fails without a change to the pool when two requested items have the same key.
    pub fn bind(
        &self,
        count: usize,
        requested: Vec<(usize, K, u64)>,
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
        let mut released = 0usize;
        for holder in &mut pool.holders {
            if holder
                .key
                .as_ref()
                .is_some_and(|k| !wanted.contains(&(k.clone(), holder.kind)))
            {
                holder.key = None;
                released += 1;
            }
        }
        let retained: HashMap<_, _> = pool
            .holders
            .iter()
            .enumerate()
            .filter_map(|(slot, h)| h.key.as_ref().map(|k| ((k.clone(), h.kind), slot)))
            .collect();
        let mut bindings = Vec::with_capacity(requested.len());
        for (index, key, kind) in requested {
            let slot = if let Some(slot) = retained.get(&(key.clone(), kind)) {
                pool.stats.retained += 1;
                *slot
            } else if let Some(slot) = pool
                .holders
                .iter()
                .position(|h| h.key.is_none() && h.kind == kind)
            {
                pool.stats.rebound += 1;
                pool.holders[slot].generation += 1;
                pool.holders[slot].key = Some(key.clone());
                slot
            } else {
                let slot = pool.holders.len();
                let id = pool.next;
                pool.next += 1;
                pool.stats.created += 1;
                pool.holders.push(Holder {
                    id,
                    kind,
                    key: Some(key.clone()),
                    generation: 0,
                });
                slot
            };
            let holder = &pool.holders[slot];
            bindings.push(RecycledItem {
                index,
                key,
                holder: holder.id,
                generation: holder.generation,
                size: size(index),
            });
        }
        let active = bindings.len();
        let mut spare = released;
        pool.holders.retain(|h| {
            h.key.is_some() || {
                let keep = spare > 0;
                spare = spare.saturating_sub(1);
                keep
            }
        });
        pool.stats.items = count;
        pool.stats.active = active;
        pool.stats.holders = pool.holders.len();
        Ok(bindings)
    }
}
