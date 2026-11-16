// SPDX-License-Identifier: GPL-3.0-only
use crate::lock::{LockPoisoned, lock_unpoisoned, wait_unpoisoned};
use crate::pending::{Fence, Pending};
use crate::{Activity, Merge};
use std::mem;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

#[must_use]
pub fn mailbox<T: Merge>() -> (Sender<T>, Receiver<T>) {
    mailbox_followed_by(None)
}

pub(crate) fn mailbox_followed_by<T: Merge>(
    activity: Option<Box<dyn Activity>>,
) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            waiting: Pending::new(Fence::EveryKey),
            senders: 1,
            receiving: true,
            closed_intake: false,
            wake: None,
            activity,
        }),
        arrived: Condvar::new(),
        opened: Condvar::new(),
    });
    let sender = Sender {
        shared: Arc::clone(&shared),
    };
    (sender, Receiver { shared })
}

pub struct Sender<T: Merge> {
    shared: Arc<Shared<T>>,
}

pub struct Receiver<T: Merge> {
    shared: Arc<Shared<T>>,
}

pub struct Waker<T: Merge> {
    shared: Arc<Shared<T>>,
}

pub struct Intake<T: Merge> {
    shared: Arc<Shared<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the other end of the link is gone")]
pub struct Closed;

impl From<LockPoisoned> for Closed {
    fn from(LockPoisoned { .. }: LockPoisoned) -> Self {
        Self
    }
}

struct Shared<T: Merge> {
    state: Mutex<State<T>>,
    arrived: Condvar,
    opened: Condvar,
}

struct State<T: Merge> {
    waiting: Pending<T>,
    senders: usize,
    receiving: bool,
    closed_intake: bool,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    activity: Option<Box<dyn Activity>>,
}

impl<T: Merge> State<T> {
    fn awaits_items(&self) -> bool {
        self.waiting.is_empty() && self.senders > 0
    }

    fn rest_if_empty(&mut self) {
        if self.waiting.is_empty()
            && let Some(activity) = &mut self.activity
        {
            activity.rest();
        }
    }

    fn work(&mut self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        if let Some(activity) = &mut self.activity {
            activity.work();
        }
        self.wake.clone()
    }
}

impl<T: Merge> Sender<T> {
    pub fn push(&self, item: T) -> Result<(), Closed> {
        let waits = !item.passes_closed_intake();
        let mut state = wait_unpoisoned(&self.shared.opened, self.shared.lock()?, |state| {
            state.receiving && state.closed_intake && waits
        })?;
        if !state.receiving {
            return Err(Closed);
        }
        let was_empty = state.waiting.is_empty();
        state.waiting.push(item);
        let wake = state.work().filter(|_| was_empty);
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
        let _poisoned_mailbox_is_closed_for_every_sender =
            self.shared.update(|state| state.senders += 1);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T: Merge> Drop for Sender<T> {
    fn drop(&mut self) {
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
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
    pub fn take(&self) -> Result<Vec<T>, Closed> {
        let mut state = self.shared.lock()?;
        state.rest_if_empty();
        let awaits_items = |state: &mut State<T>| state.awaits_items();
        let state = wait_unpoisoned(&self.shared.arrived, state, awaits_items)?;
        take_waiting(state)
    }

    pub fn take_arrived(&self) -> Result<Vec<T>, Closed> {
        take_waiting(self.shared.lock()?)
    }

    pub fn on_arrival(&self, wake: impl Fn() + Send + Sync + 'static) {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let _poisoned_mailbox_is_closed_for_the_consumer = self
            .shared
            .update(|state| state.wake = Some(Arc::clone(&wake)));
        wake();
    }

    pub fn rest_if_empty(&self) {
        let _poisoned_mailbox_is_closed_for_the_consumer = self.shared.update(State::rest_if_empty);
    }

    #[must_use]
    pub fn intake(&self) -> Intake<T> {
        Intake {
            shared: Arc::clone(&self.shared),
        }
    }

    #[must_use]
    pub fn waker(&self) -> Waker<T> {
        Waker {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T: Merge> Drop for Receiver<T> {
    fn drop(&mut self) {
        let _poisoned_mailbox_is_closed_for_every_sender = self.shared.update(|state| {
            state.receiving = false;
            state.activity = None;
        });
        self.shared.opened.notify_all();
    }
}

impl<T: Merge> Waker<T> {
    pub fn wake(&self) {
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
        let wake = state.work();
        drop(state);
        if let Some(wake) = wake {
            wake();
        }
    }
}

impl<T: Merge> Intake<T> {
    pub fn close(&self) {
        let _poisoned_mailbox_is_closed_for_every_sender =
            self.shared.update(|state| state.closed_intake = true);
    }

    pub fn open(&self) {
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
        if mem::replace(&mut state.closed_intake, false) {
            drop(state);
            self.shared.opened.notify_all();
        }
    }
}

fn take_waiting<T: Merge>(mut state: MutexGuard<'_, State<T>>) -> Result<Vec<T>, Closed> {
    if state.waiting.is_empty() && state.senders == 0 {
        return Err(Closed);
    }
    Ok(state.waiting.take())
}

impl<T: Merge> Shared<T> {
    fn lock(&self) -> Result<MutexGuard<'_, State<T>>, Closed> {
        Ok(lock_unpoisoned(&self.state)?)
    }

    fn update<R>(&self, change: impl FnOnce(&mut State<T>) -> R) -> Result<R, Closed> {
        let mut state = self.lock()?;
        Ok(change(&mut state))
    }
}

#[cfg(test)]
mod tests;
