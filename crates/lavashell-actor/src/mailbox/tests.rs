// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_items::{Item, Sum, set, step};
use std::thread;

type TestResult = Result<(), Closed>;

#[test]
fn an_overwrite_merges_until_an_item_that_does_not_overwrite_fences_every_key() -> TestResult {
    let (sender, receiver) = mailbox();
    let pushed = [
        set(0, 1.0),
        set(1, 1.0),
        set(0, 2.0),
        step(1.0),
        set(0, 3.0),
        set(1, 2.0),
        set(0, 4.0),
        Item::Fence(1),
        set(0, 5.0),
    ];
    for item in pushed {
        sender.push(item)?;
    }
    let merged = [
        set(0, 2.0),
        set(1, 1.0),
        step(1.0),
        set(0, 4.0),
        set(1, 2.0),
        Item::Fence(1),
        set(0, 5.0),
    ];
    assert_eq!(receiver.take()?, merged);
    Ok(())
}

#[test]
fn an_item_with_its_own_absorb_folds_into_the_waiting_item_of_its_key() -> TestResult {
    let (sender, receiver) = mailbox();
    let amounts = [1.0, 2.0, 4.0];
    for amount in amounts {
        sender.push(Sum { key: 0, amount })?;
    }
    let amount = amounts.iter().sum();
    assert_eq!(receiver.take()?, [Sum { key: 0, amount }]);
    Ok(())
}

#[test]
fn while_the_intake_is_closed_only_overwrites_pass_until_it_opens() -> TestResult {
    let (sender, receiver) = mailbox();
    let intake = receiver.intake();
    intake.close();
    let waiting = sender.clone();
    let pusher = thread::spawn(move || waiting.push(step(1.0)));
    sender.push(set(0, 1.0))?;
    assert_eq!(receiver.take()?, [set(0, 1.0)]);
    intake.open();
    assert_eq!(receiver.take()?, [step(1.0)]);
    assert!(matches!(pusher.join(), Ok(Ok(()))));
    Ok(())
}

#[test]
fn a_push_that_waits_fails_once_the_receiver_is_gone() {
    let (sender, receiver) = mailbox();
    receiver.intake().close();
    let pusher = thread::spawn(move || sender.push(step(1.0)));
    drop(receiver);
    assert!(matches!(pusher.join(), Ok(Err(Closed))));
}

#[test]
fn take_fails_once_every_sender_is_gone_and_no_item_waits() -> TestResult {
    let (sender, receiver) = mailbox();
    sender.push(step(1.0))?;
    drop(sender);
    assert_eq!(receiver.take()?, [step(1.0)]);
    assert_eq!(receiver.take(), Err(Closed));
    Ok(())
}
