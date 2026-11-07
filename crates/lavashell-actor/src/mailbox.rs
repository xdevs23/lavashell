// SPDX-License-Identifier: GPL-3.0-only
use crate::Merge;
use crate::lock::{lock_ignoring_poison, wait_ignoring_poison};
use crate::pending::{Fence, Pending};
use std::mem;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Makes a mailbox whose items merge while they wait for the receiver. An overwrite replaces the newest earlier item
/// with the same key, unless an item that does not overwrite arrived after it. Every other item is appended.
#[must_use]
pub fn mailbox<T: Merge>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            waiting: Pending::new(Fence::EveryKey),
            senders: 1,
            receiving: true,
            closed_intake: false,
        }),
        arrived: Condvar::new(),
        opened: Condvar::new(),
    });
    let sender = Sender {
        shared: Arc::clone(&shared),
    };
    (sender, Receiver { shared })
}

/// The end of a mailbox that pushes items. A clone pushes into the same mailbox.
pub struct Sender<T: Merge> {
    shared: Arc<Shared<T>>,
}

/// The end of a mailbox that receives the items.
pub struct Receiver<T: Merge> {
    shared: Arc<Shared<T>>,
}

/// Closes and opens the intake of a mailbox.
pub struct Intake<T: Merge> {
    shared: Arc<Shared<T>>,
}

/// The other end of a link is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the other end of the link is gone")]
pub struct Closed;

struct Shared<T: Merge> {
    state: Mutex<State<T>>,
    /// Wakes the receiver once an item arrives or every sender is gone.
    arrived: Condvar,
    /// Wakes the waiting senders once the intake opens or the receiver is gone.
    opened: Condvar,
}

struct State<T: Merge> {
    waiting: Pending<T>,
    senders: usize,
    receiving: bool,
    closed_intake: bool,
}

impl<T: Merge> Sender<T> {
    /// Pushes `item`. While the intake is closed, an item that does not pass a closed intake waits until the intake
    /// opens.
    ///
    /// # Errors
    ///
    /// Fails when the receiver is gone.
    pub fn push(&self, item: T) -> Result<(), Closed> {
        let waits = !item.passes_closed_intake();
        let mut state = wait_ignoring_poison(&self.shared.opened, self.shared.lock(), |state| {
            state.receiving && state.closed_intake && waits
        });
        if !state.receiving {
            return Err(Closed);
        }
        let was_empty = state.waiting.is_empty();
        state.waiting.push(item);
        drop(state);
        if was_empty {
            self.shared.arrived.notify_one();
        }
        Ok(())
    }
}

impl<T: Merge> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.lock().senders += 1;
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T: Merge> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.lock().senders -= 1;
        self.shared.arrived.notify_one();
    }
}

impl<T: Merge> Receiver<T> {
    /// Waits for at least one item, then removes and returns every waiting item, in order.
    ///
    /// # Errors
    ///
    /// Fails once no item waits and every sender is gone.
    pub fn take(&self) -> Result<Vec<T>, Closed> {
        let mut state = wait_ignoring_poison(&self.shared.arrived, self.shared.lock(), |state| {
            state.waiting.is_empty() && state.senders > 0
        });
        if state.waiting.is_empty() {
            return Err(Closed);
        }
        Ok(state.waiting.take())
    }

    #[must_use]
    pub fn intake(&self) -> Intake<T> {
        Intake {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T: Merge> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.lock().receiving = false;
        self.shared.opened.notify_all();
    }
}

impl<T: Merge> Intake<T> {
    /// Makes every later push of an item that does not pass a closed intake wait until the intake opens.
    pub fn close(&self) {
        self.shared.lock().closed_intake = true;
    }

    pub fn open(&self) {
        let mut state = self.shared.lock();
        if mem::replace(&mut state.closed_intake, false) {
            drop(state);
            self.shared.opened.notify_all();
        }
    }
}

impl<T: Merge> Shared<T> {
    /// Each change leaves the state consistent before code outside the mailbox runs, so it is valid after a panic.
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        lock_ignoring_poison(&self.state)
    }
}

#[cfg(test)]
mod tests;
