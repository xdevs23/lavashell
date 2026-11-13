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
            wake: None,
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
    /// Runs on the pushing thread when an item arrives in the empty mailbox and when the last sender is gone.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl<T: Merge> State<T> {
    fn awaits_items(&self) -> bool {
        self.waiting.is_empty() && self.senders > 0
    }
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
        let wake = was_empty.then(|| state.wake.clone()).flatten();
        drop(state);
        if was_empty {
            self.shared.arrived.notify_one();
        }
        if let Some(wake) = wake {
            wake();
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
        let mut state = self.shared.lock();
        state.senders -= 1;
        let wake = (state.senders == 0).then(|| state.wake.clone()).flatten();
        drop(state);
        self.shared.arrived.notify_one();
        if let Some(wake) = wake {
            wake();
        }
    }
}

impl<T: Merge> Receiver<T> {
    /// Waits for at least one item, then removes and returns every waiting item, in order.
    ///
    /// # Errors
    ///
    /// Fails once no item waits and every sender is gone.
    pub fn take(&self) -> Result<Vec<T>, Closed> {
        let awaits_items = |state: &mut State<T>| state.awaits_items();
        let state = wait_ignoring_poison(&self.shared.arrived, self.shared.lock(), awaits_items);
        take_waiting(state)
    }

    /// Removes and returns every waiting item, in order, without waiting for one. For a receiver that `on_arrival`
    /// wakes.
    ///
    /// # Errors
    ///
    /// Fails once no item waits and every sender is gone.
    pub fn take_arrived(&self) -> Result<Vec<T>, Closed> {
        take_waiting(self.shared.lock())
    }

    /// Runs `wake` on the pushing thread each time an item arrives in the empty mailbox, and once every sender is
    /// gone, so that a receiver that also waits for other sources learns that it can take items. Replaces the function
    /// set before.
    pub fn on_arrival(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.shared.lock().wake = Some(Arc::new(wake));
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

/// Removes and returns every waiting item, or fails when no item waits and every sender is gone.
fn take_waiting<T: Merge>(mut state: MutexGuard<'_, State<T>>) -> Result<Vec<T>, Closed> {
    if state.waiting.is_empty() && state.senders == 0 {
        return Err(Closed);
    }
    Ok(state.waiting.take())
}

impl<T: Merge> Shared<T> {
    /// Each change leaves the state consistent before code outside the mailbox runs, so it is valid after a panic.
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        lock_ignoring_poison(&self.state)
    }
}

#[cfg(test)]
mod tests;
