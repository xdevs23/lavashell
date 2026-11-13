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

/// What ended the wait of a [`DeadlineReceiver`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Arrival<T> {
    /// The items that waited in the mailbox, in order. There is at least one.
    Items(Vec<T>),
    /// The deadline passed while no item waited.
    DeadlinePassed,
    /// No item waits and every sender is gone.
    SendersGone,
}

/// The end of a mailbox that receives its items and also waits for a deadline on a clock. The mailbox makes one file
/// descriptor readable when an item arrives, and a timer of the system makes another readable when the deadline
/// passes on the system clock, so that one poll waits for both events. A clock that does not move by itself wakes
/// the receiver as an arrival does once it moved past the deadline.
pub(crate) struct DeadlineReceiver<T: Merge> {
    items: Receiver<T>,
    waiter: Box<dyn Waiter>,
    arrived: Arc<ArrivalSignal>,
    deadline: OwnedFd,
}

/// The file descriptor that the senders make readable when an item arrives, and the first failure to do so.
struct ArrivalSignal {
    descriptor: OwnedFd,
    failure: OnceLock<Errno>,
}

impl<T: Merge + Send + 'static> DeadlineReceiver<T>
where
    T::Key: Send,
{
    /// Receives the items of `items` and waits for deadlines on `clock`, and replaces the function run by `items` on
    /// arrival.
    ///
    /// # Errors
    ///
    /// Fails when the file descriptor that signals arrivals or the timer cannot be created.
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

    /// Removes every waiting item, or else waits until an item arrives, every sender is gone or `deadline` passes on
    /// the clock. Without a deadline, the wait ends only with an arrival or with the end of the senders.
    ///
    /// # Errors
    ///
    /// Fails when a sender could not signal an arrival, or when the timer cannot be set or the wait for the file
    /// descriptors fails.
    pub(crate) fn take_until(&self, deadline: Option<Duration>) -> Result<Arrival<T>, Error> {
        loop {
            if let Some(&failure) = self.arrived.failure.get() {
                return Err(Error::SignalInput(failure));
            }
            match self.items.take_arrived() {
                Err(Closed) => return Ok(Arrival::SendersGone),
                Ok(items) if items.is_empty() => {}
                Ok(items) => return Ok(Arrival::Items(items)),
            }
            let left = match self.waiter.wait_until(deadline) {
                Timeout::Passed => return Ok(Arrival::DeadlinePassed),
                Timeout::After(left) => Some(left),
                Timeout::Unbounded => None,
            };
            if !self.items.rest_if_empty() {
                continue;
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

    /// Makes the timer readable once `left` has passed, or never for `None`. Setting the timer also clears an
    /// expiry that was not read.
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

    /// Makes the file descriptor that signals arrivals unreadable until the next arrival.
    fn reset_signal(&self) -> Result<(), Error> {
        let mut count = [0_u8; 8];
        let _bytes = read(&self.arrived.descriptor, &mut count).map_err(Error::WaitForInput)?;
        Ok(())
    }
}

impl ArrivalSignal {
    /// Makes the descriptor readable. Runs on the thread of the sender that filled the empty mailbox or dropped the
    /// last sender, so a failure is recorded for the receiver. A full counter fails with `EAGAIN` and leaves the
    /// descriptor readable, which is the signal.
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
