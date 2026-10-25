// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn ms(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

fn logging(motion: &Motion, log: &Rc<RefCell<Vec<&'static str>>>, name: &'static str) -> Timer {
    let log = log.clone();
    motion.timer(move || log.borrow_mut().push(name))
}

#[test]
fn timers_run_in_the_order_of_their_deadlines_once_the_clock_is_at_or_past_them() {
    let motion = Motion::default();
    let log = Rc::default();
    let late = logging(&motion, &log, "late");
    let early = logging(&motion, &log, "early");
    late.start(ms(30));
    early.start(ms(20));
    assert_eq!(motion.next_deadline(), Some(ms(20)));
    motion.advance(ms(19));
    assert!(log.borrow().is_empty());
    motion.advance(ms(40));
    assert_eq!(*log.borrow(), ["early", "late"]);
    assert_eq!(motion.next_deadline(), None);
    motion.advance(ms(100));
    assert_eq!(log.borrow().len(), 2);
}

#[test]
fn a_repeating_timer_runs_once_in_an_advance_and_skips_the_deadlines_it_missed() {
    let motion = Motion::default();
    let log = Rc::default();
    let repeat = logging(&motion, &log, "repeat");
    let other = logging(&motion, &log, "other");
    repeat.repeat(ms(400), ms(50));
    other.start(ms(470));
    motion.advance(ms(399));
    assert!(log.borrow().is_empty());
    motion.advance(ms(400));
    assert_eq!(*log.borrow(), ["repeat"]);
    motion.advance(ms(533));
    assert_eq!(*log.borrow(), ["repeat", "repeat", "other"]);
    assert_eq!(motion.next_deadline(), Some(ms(550)));
    motion.advance(ms(550));
    assert_eq!(log.borrow().len(), 4);
    repeat.cancel();
    motion.advance(ms(1000));
    assert_eq!(log.borrow().len(), 4);
}

#[test]
fn a_repeating_timer_without_an_interval_runs_once_in_each_advance() {
    let motion = Motion::default();
    let log = Rc::default();
    let repeat = logging(&motion, &log, "repeat");
    repeat.repeat(ms(10), Duration::ZERO);
    motion.advance(ms(10));
    assert_eq!(motion.next_deadline(), Some(ms(10)));
    motion.advance(ms(11));
    motion.advance(ms(11));
    assert_eq!(log.borrow().len(), 3);
}

#[test]
fn starting_a_pending_timer_moves_its_deadline_and_dropping_it_cancels_it() {
    let motion = Motion::default();
    let log = Rc::default();
    let timer = logging(&motion, &log, "moved");
    timer.start(ms(50));
    motion.advance(ms(30));
    timer.start(ms(50));
    motion.advance(ms(79));
    assert!(log.borrow().is_empty());
    motion.advance(ms(80));
    assert_eq!(*log.borrow(), ["moved"]);
    let dropped = logging(&motion, &log, "dropped");
    dropped.start(ms(10));
    drop(dropped);
    assert_eq!(motion.next_deadline(), None);
    motion.advance(ms(200));
    assert_eq!(*log.borrow(), ["moved"]);
}

#[test]
fn a_timer_started_by_a_running_timer_waits_for_the_next_advance() {
    let motion = Motion::default();
    let log = Rc::default();
    let second = logging(&motion, &log, "second");
    let first = {
        let (log, second) = (log.clone(), second.clone());
        motion.timer(move || {
            log.borrow_mut().push("first");
            second.start(Duration::ZERO);
        })
    };
    first.start(ms(10));
    motion.advance(ms(10));
    assert_eq!(*log.borrow(), ["first"]);
    assert!(motion.timer_due());
    motion.advance(ms(10));
    assert_eq!(*log.borrow(), ["first", "second"]);
}

#[test]
fn reduced_motion_does_not_change_the_deadline_of_a_timer() {
    let motion = Motion::default();
    let log = Rc::default();
    let timer = logging(&motion, &log, "linger");
    timer.start(ms(70));
    motion.reduced_motion(true);
    motion.advance(ms(69));
    assert!(log.borrow().is_empty());
    motion.advance(ms(70));
    assert_eq!(*log.borrow(), ["linger"]);
}

#[test]
fn a_time_source_makes_a_timer_due_between_advances() {
    let motion = Motion::default();
    let clock = Rc::new(Cell::new(ms(100)));
    let time = clock.clone();
    motion.time_source(move || time.get());
    let log = Rc::default();
    let timer = logging(&motion, &log, "due");
    timer.start(ms(70));
    assert_eq!(motion.next_deadline(), Some(ms(170)));
    assert!(!motion.timer_due());
    clock.set(ms(170));
    assert!(motion.timer_due());
    motion.advance(ms(170));
    assert_eq!(*log.borrow(), ["due"]);
}
