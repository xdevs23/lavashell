// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{
    Activity, Clock, Timeout, Waiter, lock_ignoring_poison, wait_ignoring_poison,
};
use std::collections::BTreeMap;
use std::mem;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// A clock that does not move until `advance_to` is called. It counts the receivers of the actors that run on it
/// while they have work, so the script can wait until those actors have nothing left to do at the current time.
#[derive(Clone, Default)]
pub(crate) struct VirtualClock(Arc<Shared>);

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Wakes the script once no receiver has work.
    idle: Condvar,
}

#[derive(Default)]
struct State {
    now: Duration,
    /// The number of receivers on the clock that have work.
    busy: usize,
    /// The deadline of each waiter that waits for one.
    deadlines: BTreeMap<u64, Deadline>,
    next_waiter: u64,
}

struct Deadline {
    time: Duration,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// A receiver registered as waiting on a virtual clock.
struct VirtualWaiter {
    clock: Arc<Shared>,
    id: u64,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// Counts a receiver as busy from its creation, and again from each arrival, until it rests.
struct ReceiverActivity {
    clock: Arc<Shared>,
    busy: bool,
}

impl VirtualClock {
    /// Moves the clock to `time`, unless the clock is past it already, and wakes every waiter whose deadline is no
    /// later than the new time.
    pub(crate) fn advance_to(&self, time: Duration) {
        let mut state = lock_ignoring_poison(&self.0.state);
        state.now = state.now.max(time);
        let now = state.now;
        let (due, ahead): (BTreeMap<_, _>, _) = (mem::take(&mut state.deadlines).into_iter())
            .partition(|(_, deadline)| deadline.time <= now);
        state.deadlines = ahead;
        drop(state);
        for deadline in due.into_values() {
            (deadline.wake)();
        }
    }

    /// Waits until no receiver on the clock has work, and returns the earliest deadline of the waiters then, or
    /// `None` when no waiter waits for a deadline.
    pub(crate) fn next_deadline_when_idle(&self) -> Option<Duration> {
        let state = lock_ignoring_poison(&self.0.state);
        let state = wait_ignoring_poison(&self.0.idle, state, |state| state.busy > 0);
        state.deadlines.values().map(|deadline| deadline.time).min()
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Duration {
        lock_ignoring_poison(&self.0.state).now
    }

    fn waiter(&self, wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn Waiter> {
        let mut state = lock_ignoring_poison(&self.0.state);
        let id = state.next_waiter;
        state.next_waiter += 1;
        Box::new(VirtualWaiter {
            clock: Arc::clone(&self.0),
            id,
            wake,
        })
    }

    fn activity(&self) -> Option<Box<dyn Activity>> {
        self.0.begin();
        Some(Box::new(ReceiverActivity {
            clock: Arc::clone(&self.0),
            busy: true,
        }))
    }
}

impl Waiter for VirtualWaiter {
    /// Records `deadline` while it is ahead of the clock. The receiver then waits for an arrival, which the advance
    /// past the deadline makes.
    fn wait_until(&self, deadline: Option<Duration>) -> Timeout {
        let mut state = lock_ignoring_poison(&self.clock.state);
        let ahead = deadline.filter(|&time| time > state.now);
        let _earlier = match ahead {
            Some(time) => {
                let wake = Arc::clone(&self.wake);
                state.deadlines.insert(self.id, Deadline { time, wake })
            }
            None => state.deadlines.remove(&self.id),
        };
        if ahead == deadline {
            Timeout::Unbounded
        } else {
            Timeout::Passed
        }
    }
}

impl Drop for VirtualWaiter {
    fn drop(&mut self) {
        let _deadline = lock_ignoring_poison(&self.clock.state)
            .deadlines
            .remove(&self.id);
    }
}

impl Activity for ReceiverActivity {
    fn work(&mut self) {
        if !mem::replace(&mut self.busy, true) {
            self.clock.begin();
        }
    }

    fn rest(&mut self) {
        if mem::replace(&mut self.busy, false) {
            self.clock.end();
        }
    }
}

impl Drop for ReceiverActivity {
    fn drop(&mut self) {
        self.rest();
    }
}

impl Shared {
    fn begin(&self) {
        lock_ignoring_poison(&self.state).busy += 1;
    }

    fn end(&self) {
        let mut state = lock_ignoring_poison(&self.state);
        state.busy -= 1;
        if state.busy == 0 {
            self.idle.notify_all();
        }
    }
}

#[cfg(test)]
mod tests;
