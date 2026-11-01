// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::panic::resume_unwind;
use std::sync::Barrier;
use std::thread;

#[test]
fn a_slot_reads_nothing_before_the_first_publish() {
    assert_eq!(Latest::<u64>::new().read(), None);
}

#[test]
fn a_publish_replaces_the_value_before_it() {
    let latest = Latest::new();
    latest.publish(1);
    latest.publish(2);
    assert_eq!(latest.read().as_deref(), Some(&2));
}

#[test]
fn a_lease_stays_valid_after_a_newer_publish_and_after_a_clear() {
    let latest = Latest::new();
    latest.publish(1);
    let lease = latest.read();
    latest.publish(2);
    assert_eq!(lease.as_deref(), Some(&1));
    let newer = latest.read();
    latest.clear();
    assert_eq!(latest.read(), None);
    assert_eq!(newer.as_deref(), Some(&2));
}

#[test]
fn a_reader_beside_a_publisher_reads_neither_a_stale_nor_an_unpublished_value() {
    const PUBLISHES: u64 = 100;
    let latest = Latest::new();
    let round = Barrier::new(2);
    let reads: Vec<_> = thread::scope(|scope| {
        let _publisher = scope.spawn(|| {
            for value in 0..PUBLISHES {
                latest.publish(value);
                let _leader = round.wait();
            }
        });
        (0..PUBLISHES)
            .map(|_| {
                let _leader = round.wait();
                latest.read()
            })
            .collect()
    });
    for (published, read) in (0..PUBLISHES).zip(reads) {
        assert!(read.is_some_and(|value| (published..=published + 1).contains(&*value)));
    }
}

#[test]
fn a_slot_poisoned_by_a_panicking_thread_still_publishes_reads_and_clears() {
    let latest = Latest::new();
    latest.publish(1);
    let poisoner = thread::scope(|scope| {
        scope
            .spawn(|| {
                let _guard = latest.lock();
                resume_unwind(Box::new("the slot is held while the thread panics"));
            })
            .join()
    });
    assert!(poisoner.is_err());
    assert!(latest.slot.is_poisoned());
    assert_eq!(latest.read().as_deref(), Some(&1));
    latest.publish(2);
    assert_eq!(latest.read().as_deref(), Some(&2));
    latest.clear();
    assert_eq!(latest.read(), None);
}
