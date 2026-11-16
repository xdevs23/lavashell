// SPDX-License-Identifier: GPL-3.0-only
use super::{Cell, Duration, Motion, Rc, RefCell, Weak};
use crate::numbers::duration_from_nanos;

#[derive(Clone, Copy)]
struct Schedule {
    deadline: Duration,
    interval: Option<Duration>,
    armed: u64,
}

pub(super) struct TimerState {
    fire: RefCell<Box<dyn FnMut()>>,
    schedule: Cell<Option<Schedule>>,
}

#[derive(Clone)]
pub struct Timer {
    state: Rc<TimerState>,
    clock: Motion,
}

impl Timer {
    pub fn start(&self, delay: Duration) {
        self.schedule(delay, None);
    }

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

    pub(super) fn current(&self) -> Duration {
        let now = self.0.now.get();
        (self.0.source.borrow().as_ref()).map_or(now, |source| source().max(now))
    }

    fn started_timers(&self) -> Vec<(Schedule, Rc<TimerState>)> {
        let mut timers = self.0.timers.borrow_mut();
        timers.retain(|timer| timer.strong_count() > 0);
        (timers.iter().filter_map(Weak::upgrade))
            .filter_map(|timer| Some((timer.schedule.get()?, timer)))
            .collect()
    }

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
