// SPDX-License-Identifier: GPL-3.0-only
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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

    /// Ignores lock poisoning, because the slot is replaced as a whole and is valid after a panic.
    fn lock(&self) -> MutexGuard<'_, Option<Arc<T>>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
