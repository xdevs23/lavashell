// SPDX-License-Identifier: GPL-3.0-only
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, also after a thread panicked while it held the lock. Only for a value that each change leaves valid,
/// so that a panic in the middle of a change leaves no broken value behind.
pub fn lock_ignoring_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits on `signal` while `condition` holds, with the same rule for a panicked holder as [`lock_ignoring_poison`].
pub fn wait_ignoring_poison<'a, T>(
    signal: &Condvar,
    guard: MutexGuard<'a, T>,
    condition: impl FnMut(&mut T) -> bool,
) -> MutexGuard<'a, T> {
    (signal.wait_while(guard, condition)).unwrap_or_else(PoisonError::into_inner)
}
