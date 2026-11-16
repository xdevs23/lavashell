// SPDX-License-Identifier: GPL-3.0-only
use std::any::type_name;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Debug, thiserror::Error)]
#[error("the lock around {guarded} is poisoned: a thread panicked while it held the lock")]
pub struct LockPoisoned {
    pub guarded: &'static str,
}

pub fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, LockPoisoned> {
    mutex.lock().map_err(|_poisoned| LockPoisoned {
        guarded: type_name::<T>(),
    })
}

pub fn wait_unpoisoned<'a, T>(
    signal: &Condvar,
    guard: MutexGuard<'a, T>,
    condition: impl FnMut(&mut T) -> bool,
) -> Result<MutexGuard<'a, T>, LockPoisoned> {
    signal
        .wait_while(guard, condition)
        .map_err(|_poisoned| LockPoisoned {
            guarded: type_name::<T>(),
        })
}

pub fn lock_ignoring_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn wait_ignoring_poison<'a, T>(
    signal: &Condvar,
    guard: MutexGuard<'a, T>,
    condition: impl FnMut(&mut T) -> bool,
) -> MutexGuard<'a, T> {
    (signal.wait_while(guard, condition)).unwrap_or_else(PoisonError::into_inner)
}
