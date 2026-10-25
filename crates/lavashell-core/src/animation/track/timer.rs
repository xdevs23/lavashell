// SPDX-License-Identifier: GPL-3.0-only
use super::{Cell, Duration, Motion, Rc, RefCell, Weak};
use crate::numbers::duration_from_nanos;

#[derive(Clone, Copy)]
struct Schedule {
    deadline: Duration,
    interval: Option<Duration>,
    /// The number of advances of the clock when the timer was started or ran last. The timer runs
    /// only in a later advance.
    armed: u64,
}

pub(super) struct TimerState {
    fire: RefCell<Box<dyn FnMut()>>,
    schedule: Cell<Option<Schedule>>,
}

/// A callback run by a motion clock at a deadline, once or repeatedly. Reduced motion does not
/// change when the callback runs. Dropping every clone of a timer cancels the timer.
#[derive(Clone)]
pub struct Timer {
    state: Rc<TimerState>,
    clock: Motion,
}

impl Timer {
    /// Runs the callback once, `delay` after the current time of the clock. Starting a pending
    /// timer moves its deadline.
    pub fn start(&self, delay: Duration) {
        self.schedule(delay, None);
    }

    /// Runs the callback `delay` after the current time of the clock, then every `interval`, at
    /// most once each time the clock advances. When the clock advances past several deadlines,
    /// the callback runs once and the other deadlines are skipped. With an interval of zero, the
    /// callback runs each time the clock advances.
    pub fn repeat(&self, delay: Duration, interval: Duration) {
        self.schedule(delay, Some(interval));
    }

    pub fn cancel(&self) {
        self.state.schedule.set(None);
    }

    fn schedule(&self, delay: Duration, interval: Option<Duration>) {
        self.state.schedule.set(Some(Schedule {
            deadline: self.clock.current() + delay,
            interval,
            armed: self.clock.0.advances.get(),
        }));
    }
}

impl Motion {
    pub fn timer(&self, fire: impl FnMut() + 'static) -> Timer {
        let state = Rc::new(TimerState {
            fire: RefCell::new(Box::new(fire)),
            schedule: Cell::new(None),
        });
        self.0.timers.borrow_mut().push(Rc::downgrade(&state));
        Timer {
            state,
            clock: self.clone(),
        }
    }

    /// The earliest deadline of a started timer, on the epoch of `advance`.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        (self.started_timers().iter())
            .map(|(schedule, _)| schedule.deadline)
            .min()
    }

    pub(crate) fn timer_due(&self) -> bool {
        self.next_deadline()
            .is_some_and(|deadline| deadline <= self.current())
    }

    /// The time of the last advance, or the time returned by the time source when that time is
    /// later.
    pub(super) fn current(&self) -> Duration {
        let now = self.0.now.get();
        (self.0.source.borrow().as_ref()).map_or(now, |source| source().max(now))
    }

    /// The schedule of every started timer, in the order the timers were created.
    fn started_timers(&self) -> Vec<(Schedule, Rc<TimerState>)> {
        let mut timers = self.0.timers.borrow_mut();
        timers.retain(|timer| timer.strong_count() > 0);
        (timers.iter().filter_map(Weak::upgrade))
            .filter_map(|timer| Some((timer.schedule.get()?, timer)))
            .collect()
    }

    /// Runs the timers armed before this advance whose deadline is not after the time of the
    /// clock, in the order of their deadlines.
    pub(super) fn fire_timers(&self) {
        let advance = self.0.advances.get() + 1;
        self.0.advances.set(advance);
        let now = self.0.now.get();
        while let Some((schedule, timer)) = (self.started_timers().into_iter())
            .filter(|(schedule, _)| schedule.deadline <= now && schedule.armed < advance)
            .min_by_key(|(schedule, _)| schedule.deadline)
        {
            timer
                .schedule
                .set(schedule.interval.map(|interval| Schedule {
                    deadline: next_deadline(schedule.deadline, interval, now),
                    interval: Some(interval),
                    armed: advance,
                }));
            (timer.fire.borrow_mut())();
        }
    }
}

/// The first deadline after `now` in steps of `interval` from `deadline`, or `now` for an interval
/// of zero.
fn next_deadline(deadline: Duration, interval: Duration, now: Duration) -> Duration {
    let Some(rest) = now
        .saturating_sub(deadline)
        .as_nanos()
        .checked_rem(interval.as_nanos())
    else {
        return now;
    };
    (now + interval).saturating_sub(duration_from_nanos(rest))
}
