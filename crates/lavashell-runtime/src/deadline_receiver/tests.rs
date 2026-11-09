// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Effect, mailbox};
use std::thread;

#[derive(Debug, thiserror::Error)]
enum DeadlineReceiverTestError {
    #[error(transparent)]
    DeadlineReceiver(#[from] Error),
    #[error("a mailbox of the test closed")]
    Closed(#[from] Closed),
}

type DeadlineReceiverTestResult = Result<(), DeadlineReceiverTestError>;

/// A number in a mailbox, where each number builds on the numbers before it.
#[derive(Debug, PartialEq)]
struct Number(u32);

impl Merge for Number {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}

#[test]
fn items_that_waited_before_the_receiver_existed_are_returned() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox();
    sender.push(Number(1))?;
    sender.push(Number(2))?;
    drop(sender);
    let receiver = DeadlineReceiver::new(items)?;
    assert_eq!(
        receiver.take_until(None)?,
        Arrival::Items(vec![Number(1), Number(2)])
    );
    assert_eq!(receiver.take_until(None)?, Arrival::SendersGone);
    Ok(())
}

#[test]
fn an_item_that_arrives_during_the_wait_ends_it() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox();
    let receiver = DeadlineReceiver::new(items)?;
    let pusher = thread::spawn(move || sender.push(Number(1)));
    assert_eq!(receiver.take_until(None)?, Arrival::Items(vec![Number(1)]));
    assert!(matches!(pusher.join(), Ok(Ok(()))));
    Ok(())
}

#[test]
fn dropping_the_last_sender_ends_the_wait() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox::<Number>();
    let receiver = DeadlineReceiver::new(items)?;
    let dropper = thread::spawn(move || drop(sender));
    assert_eq!(receiver.take_until(None)?, Arrival::SendersGone);
    assert!(dropper.join().is_ok());
    Ok(())
}

#[test]
fn a_deadline_that_passed_ends_the_wait_without_an_item() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox::<Number>();
    let receiver = DeadlineReceiver::new(items)?;
    assert_eq!(
        receiver.take_until(Some(Instant::now()))?,
        Arrival::DeadlinePassed
    );
    drop(sender);
    Ok(())
}

#[test]
fn an_item_that_waits_is_returned_although_the_deadline_passed() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox();
    let receiver = DeadlineReceiver::new(items)?;
    sender.push(Number(1))?;
    assert_eq!(
        receiver.take_until(Some(Instant::now()))?,
        Arrival::Items(vec![Number(1)])
    );
    Ok(())
}

#[test]
fn a_wait_after_a_taken_item_ends_at_its_deadline() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox();
    let receiver = DeadlineReceiver::new(items)?;
    sender.push(Number(1))?;
    assert_eq!(receiver.take_until(None)?, Arrival::Items(vec![Number(1)]));
    assert_eq!(
        receiver.take_until(Some(Instant::now()))?,
        Arrival::DeadlinePassed
    );
    Ok(())
}

#[test]
fn a_deadline_ahead_ends_the_wait_once_it_passed() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox::<Number>();
    let receiver = DeadlineReceiver::new(items)?;
    let deadline = Instant::now() + Duration::from_millis(1);
    assert_eq!(
        receiver.take_until(Some(deadline))?,
        Arrival::DeadlinePassed
    );
    assert!(Instant::now() >= deadline);
    drop(sender);
    Ok(())
}

#[test]
fn an_item_that_arrives_before_a_deadline_ahead_ends_the_wait() -> DeadlineReceiverTestResult {
    let (sender, items) = mailbox();
    let receiver = DeadlineReceiver::new(items)?;
    let pusher = thread::spawn(move || sender.push(Number(1)));
    let deadline = Instant::now() + Duration::from_secs(3600);
    assert_eq!(
        receiver.take_until(Some(deadline))?,
        Arrival::Items(vec![Number(1)])
    );
    assert!(matches!(pusher.join(), Ok(Ok(()))));
    Ok(())
}
