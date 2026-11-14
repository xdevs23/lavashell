// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Closed, Effect, Merge, mailbox_on};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const SECOND: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq)]
struct Number(u32);

impl Merge for Number {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}

/// A waiter on `clock` with the number of times it was woken.
fn counted_waiter(clock: &VirtualClock) -> (Box<dyn Waiter>, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&wakes);
    let waiter = clock.waiter(Arc::new(move || {
        let _before = counted.fetch_add(1, Ordering::SeqCst);
    }));
    (waiter, wakes)
}

#[test]
fn a_clock_without_receivers_is_idle() {
    assert_eq!(VirtualClock::default().next_deadline_when_idle(), None);
}

#[test]
fn the_clock_moves_only_forward() {
    let clock = VirtualClock::default();
    clock.advance_to(SECOND);
    clock.advance_to(Duration::ZERO);
    assert_eq!(clock.now(), SECOND);
}

#[test]
fn the_idle_clock_returns_the_earliest_deadline_of_its_waiters() {
    let clock = VirtualClock::default();
    let (later, _) = counted_waiter(&clock);
    let (earlier, _) = counted_waiter(&clock);
    let (unbounded, _) = counted_waiter(&clock);
    assert_eq!(later.wait_until(Some(2 * SECOND)), Timeout::Unbounded);
    assert_eq!(earlier.wait_until(Some(SECOND)), Timeout::Unbounded);
    assert_eq!(unbounded.wait_until(None), Timeout::Unbounded);
    assert_eq!(clock.next_deadline_when_idle(), Some(SECOND));
}

#[test]
fn an_advance_wakes_only_the_waiters_whose_deadline_passed() {
    let clock = VirtualClock::default();
    let (due, due_wakes) = counted_waiter(&clock);
    let (ahead, ahead_wakes) = counted_waiter(&clock);
    assert_eq!(due.wait_until(Some(SECOND)), Timeout::Unbounded);
    assert_eq!(ahead.wait_until(Some(2 * SECOND)), Timeout::Unbounded);
    clock.advance_to(SECOND);
    assert_eq!(due_wakes.load(Ordering::SeqCst), 1);
    assert_eq!(ahead_wakes.load(Ordering::SeqCst), 0);
    assert_eq!(clock.next_deadline_when_idle(), Some(2 * SECOND));
    assert_eq!(due.wait_until(Some(SECOND)), Timeout::Passed);
}

#[test]
fn a_dropped_waiter_leaves_the_clock() {
    let clock = VirtualClock::default();
    let (earlier, _) = counted_waiter(&clock);
    let (later, _) = counted_waiter(&clock);
    assert_eq!(earlier.wait_until(Some(SECOND)), Timeout::Unbounded);
    assert_eq!(later.wait_until(Some(2 * SECOND)), Timeout::Unbounded);
    drop(earlier);
    assert_eq!(clock.next_deadline_when_idle(), Some(2 * SECOND));
}

#[test]
fn a_receiver_is_idle_once_it_rests_with_no_item_waiting() -> Result<(), Closed> {
    let clock = VirtualClock::default();
    let (sender, receiver) = mailbox_on(&clock);
    sender.push(Number(1))?;
    assert!(!receiver.rest_if_empty());
    assert_eq!(receiver.take_arrived()?, [Number(1)]);
    assert!(receiver.rest_if_empty());
    assert_eq!(clock.next_deadline_when_idle(), None);
    Ok(())
}

#[test]
fn a_receiver_that_waits_in_take_is_idle_until_an_item_arrives() -> Result<(), Closed> {
    let clock = VirtualClock::default();
    let (sender, receiver) = mailbox_on(&clock);
    let taking = thread::spawn(move || receiver.take());
    assert_eq!(clock.next_deadline_when_idle(), None);
    sender.push(Number(1))?;
    assert!(matches!(taking.join(), Ok(Ok(items)) if items == [Number(1)]));
    Ok(())
}

#[test]
fn a_dropped_receiver_stops_counting_also_with_items_waiting() -> Result<(), Closed> {
    let clock = VirtualClock::default();
    let (sender, receiver) = mailbox_on(&clock);
    sender.push(Number(1))?;
    drop(receiver);
    assert_eq!(clock.next_deadline_when_idle(), None);
    Ok(())
}
