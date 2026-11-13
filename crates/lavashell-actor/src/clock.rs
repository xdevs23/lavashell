// SPDX-License-Identifier: GPL-3.0-only
use crate::mailbox::mailbox_followed_by;
use crate::{Merge, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The clock of a group of actors, which measures every time from the start of the clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;

    /// Registers a receiver that waits for deadlines on the clock. A clock that does not move by itself runs `wake`
    /// once it moved past the deadline of the waiter.
    fn waiter(&self, wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn Waiter>;

    /// The activity that counts whether the receiver of a mailbox made by [`mailbox_on`] has work, or `None` on a
    /// clock that does not count work.
    fn activity(&self) -> Option<Box<dyn Activity>> {
        None
    }
}

/// A receiver registered as waiting on a clock. Dropping the waiter removes it from the clock.
pub trait Waiter: Send {
    /// Waits for `deadline`, or only for an arrival when `deadline` is `None`. Returns how long the receiver waits.
    fn wait_until(&self, deadline: Option<Duration>) -> Timeout;
}

/// Whether the receiver of a mailbox has work. The mailbox calls `work` for each arrival and `rest` when the
/// receiver waits while no item is waiting for it. Dropping the activity counts as a rest.
pub trait Activity: Send {
    fn work(&mut self);
    fn rest(&mut self);
}

/// How long an actor waits for a deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    Passed,
    After(Duration),
    /// The wait ends only with an arrival.
    Unbounded,
}

/// The monotonic clock of the system, which passes each deadline by itself.
#[derive(Clone, Copy)]
pub struct SystemClock(Instant);

/// Makes a mailbox for an actor that runs on `clock`, whose receiver is counted by [`Clock::activity`].
#[must_use]
pub fn mailbox_on<T: Merge>(clock: &dyn Clock) -> (Sender<T>, Receiver<T>) {
    mailbox_followed_by(clock.activity())
}

impl SystemClock {
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn waiter(
        &self,
        _deadlines_pass_by_themselves: Arc<dyn Fn() + Send + Sync>,
    ) -> Box<dyn Waiter> {
        Box::new(*self)
    }
}

impl Waiter for SystemClock {
    fn wait_until(&self, deadline: Option<Duration>) -> Timeout {
        let Some(deadline) = deadline else {
            return Timeout::Unbounded;
        };
        match deadline
            .checked_sub(self.now())
            .filter(|left| !left.is_zero())
        {
            Some(left) => Timeout::After(left),
            None => Timeout::Passed,
        }
    }
}

#[cfg(test)]
mod tests;
