// SPDX-License-Identifier: GPL-3.0-only
use crate::lock_ignoring_poison;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard};

/// A slot that stores the newest value published by any thread. Publishing replaces the previous value, whether or
/// not anyone read it.
pub struct Latest<T> {
    slot: Mutex<Option<Arc<T>>>,
}

impl<T> Latest<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    pub fn publish(&self, value: T) {
        self.replace(Some(Arc::new(value)));
    }

    /// The newest value. The returned lease remains usable after newer publishes and after `clear`.
    #[must_use]
    pub fn read(&self) -> Option<Arc<T>> {
        self.lock().clone()
    }

    pub fn clear(&self) {
        self.replace(None);
    }

    fn replace(&self, value: Option<Arc<T>>) {
        let _replaced = mem::replace(&mut *self.lock(), value);
    }

    /// The slot is replaced as a whole, so it is valid after a panic.
    fn lock(&self) -> MutexGuard<'_, Option<Arc<T>>> {
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
