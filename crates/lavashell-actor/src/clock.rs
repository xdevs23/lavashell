// SPDX-License-Identifier: GPL-3.0-only
use crate::mailbox::mailbox_followed_by;
use crate::{Merge, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;

    fn waiter(&self, wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn Waiter>;

    fn activity(&self) -> Option<Box<dyn Activity>> {
        None
    }
}

pub trait Waiter: Send {
    fn wait_until(&self, deadline: Option<Duration>) -> Timeout;
}

pub trait Activity: Send {
    fn work(&mut self);
    fn rest(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    Passed,
    After(Duration),
    Unbounded,
}

#[derive(Clone, Copy)]
pub struct SystemClock(Instant);

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
