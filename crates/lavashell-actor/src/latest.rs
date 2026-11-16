// SPDX-License-Identifier: GPL-3.0-only
use crate::lock_ignoring_poison;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Latest<T> {
    slot: Mutex<Slot<T>>,
}

struct Slot<T> {
    value: Option<Arc<T>>,
    generation: u64,
}

pub enum Publication<T> {
    Stored,
    Replaced(Arc<T>),
    Stale,
}

impl<T> Latest<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(Slot {
                value: None,
                generation: 0,
            }),
        }
    }

    pub fn publish(&self, value: T) {
        self.publish_shared(Arc::new(value));
    }

    pub fn publish_shared(&self, value: Arc<T>) {
        self.replace(Some(value));
    }

    pub fn publish_newer(&self, generation: u64, value: T) -> Publication<T> {
        let mut slot = self.lock();
        if generation <= slot.generation {
            return Publication::Stale;
        }
        slot.generation = generation;
        match slot.value.replace(Arc::new(value)) {
            Some(replaced) => Publication::Replaced(replaced),
            None => Publication::Stored,
        }
    }

    #[must_use]
    pub fn read(&self) -> Option<Arc<T>> {
        self.lock().value.clone()
    }

    #[must_use]
    pub fn take(&self) -> Option<Arc<T>> {
        self.lock().value.take()
    }

    pub fn clear(&self) {
        self.replace(None);
    }

    fn replace(&self, value: Option<Arc<T>>) {
        let _old_value_drops_after_the_lock_is_released =
            mem::replace(&mut self.lock().value, value);
    }

    fn lock(&self) -> MutexGuard<'_, Slot<T>> {
        lock_ignoring_poison(&self.slot)
    }
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
