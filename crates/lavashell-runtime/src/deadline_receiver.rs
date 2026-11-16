// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{Clock, Closed, Merge, Receiver, Timeout, Waiter};
use rustix::event::{EventfdFlags, PollFd, PollFlags, eventfd, poll};
use rustix::io::{Errno, read, write};
use rustix::time::{
    Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, timerfd_create,
    timerfd_settime,
};
use std::os::fd::OwnedFd;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

mod error;

pub use error::Error;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Arrival<T> {
    Items(Vec<T>),
    DeadlinePassed,
    SendersGone,
}

pub(crate) struct DeadlineReceiver<T: Merge> {
    items: Receiver<T>,
    waiter: Box<dyn Waiter>,
    arrived: Arc<ArrivalSignal>,
    deadline: OwnedFd,
}

struct ArrivalSignal {
    descriptor: OwnedFd,
    failure: OnceLock<Errno>,
}

impl<T: Merge + Send + 'static> DeadlineReceiver<T>
where
    T::Key: Send,
{
    pub(crate) fn new(items: Receiver<T>, clock: &dyn Clock) -> Result<Self, Error> {
        let flags = EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK;
        let descriptor = eventfd(0, flags).map_err(Error::CreateInputWake)?;
        let arrived = Arc::new(ArrivalSignal {
            descriptor,
            failure: OnceLock::new(),
        });
        let timer_flags = TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK;
        let deadline = timerfd_create(TimerfdClockId::Monotonic, timer_flags)
            .map_err(Error::CreateInputWake)?;
        let signalled = Arc::clone(&arrived);
        items.on_arrival(move || signalled.signal());
        let waker = items.waker();
        let waiter = clock.waiter(Arc::new(move || waker.wake()));
        Ok(Self {
            items,
            waiter,
            arrived,
            deadline,
        })
    }

    pub(crate) fn take_until(&self, deadline: Option<Duration>) -> Result<Arrival<T>, Error> {
        loop {
            if let Some(&failure) = self.arrived.failure.get() {
                return Err(Error::SignalInput(failure));
            }
            if let Some(arrival) = self.take_arrival() {
                return Ok(arrival);
            }
            let left = match self.waiter.wait_until(deadline) {
                Timeout::Passed => return Ok(Arrival::DeadlinePassed),
                Timeout::After(left) => Some(left),
                Timeout::Unbounded => None,
            };
            self.items.rest_if_empty();
            if let Some(arrival) = self.take_arrival() {
                return Ok(arrival);
            }
            self.arm(left)?;
            let mut waits = [
                PollFd::new(&self.arrived.descriptor, PollFlags::IN),
                PollFd::new(&self.deadline, PollFlags::IN),
            ];
            match poll(&mut waits, None) {
                Ok(_ready) if waits[0].revents().contains(PollFlags::IN) => self.reset_signal()?,
                Ok(_) | Err(Errno::INTR) => {}
                Err(error) => return Err(Error::WaitForInput(error)),
            }
        }
    }

    fn take_arrival(&self) -> Option<Arrival<T>> {
        match self.items.take_arrived() {
            Err(Closed) => Some(Arrival::SendersGone),
            Ok(items) if items.is_empty() => None,
            Ok(items) => Some(Arrival::Items(items)),
        }
    }

    fn arm(&self, left: Option<Duration>) -> Result<(), Error> {
        let it_value = left.map_or(
            Ok(Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
            Timespec::try_from,
        );
        let value = Itimerspec {
            it_interval: Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: it_value.map_err(Error::InputDeadline)?,
        };
        let _previous = timerfd_settime(&self.deadline, TimerfdTimerFlags::empty(), &value)
            .map_err(Error::WaitForInput)?;
        Ok(())
    }

    fn reset_signal(&self) -> Result<(), Error> {
        let mut count = [0_u8; 8];
        let _bytes = read(&self.arrived.descriptor, &mut count).map_err(Error::WaitForInput)?;
        Ok(())
    }
}

impl ArrivalSignal {
    fn signal(&self) {
        match write(&self.descriptor, &1_u64.to_ne_bytes()) {
            Ok(_) | Err(Errno::AGAIN) => {}
            Err(error) => {
                let _first_failure_kept = self.failure.set(error);
            }
        }
    }
}

#[cfg(test)]
mod tests;
