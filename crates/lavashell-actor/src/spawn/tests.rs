// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{Closed, Effect, mailbox};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the test actor could not start")]
    Spawn(#[from] SpawnError<Failed>),
    #[error("the test actor stopped with a failure")]
    Join(#[from] JoinError<Failed>),
    #[error("the test actor is gone")]
    Push(#[from] Closed),
}

type TestResult = Result<(), TestError>;

#[derive(Debug, thiserror::Error)]
#[error("the test actor failed")]
struct Failed;

enum Input {
    Value(u8),
    Fail,
    Stop,
}

impl Merge for Input {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}

/// Sends each value that it handles to `seen`.
struct Echo {
    seen: mpsc::Sender<u8>,
}

impl Actor for Echo {
    type Input = Input;
    type Error = Failed;

    fn handle(&mut self, input: Input) -> Result<Flow, Failed> {
        match input {
            Input::Value(value) => {
                let _seen = self.seen.send(value);
                Ok(Flow::Continue)
            }
            Input::Fail => Err(Failed),
            Input::Stop => Ok(Flow::Stop),
        }
    }
}

#[test]
fn an_actor_takes_its_inputs_in_order_until_it_stops() -> TestResult {
    let (sender, inbox) = mailbox();
    let (seen, values) = mpsc::channel();
    let actor = spawn("echo", inbox, move || Ok(Echo { seen }))?;
    for input in [Input::Value(1), Input::Value(2), Input::Stop] {
        sender.push(input)?;
    }
    actor.join()?;
    assert_eq!(values.iter().collect::<Vec<_>>(), [1, 2]);
    Ok(())
}

#[test]
fn an_actor_stops_once_every_sender_is_gone() -> TestResult {
    let (sender, inbox) = mailbox();
    let (seen, _values) = mpsc::channel();
    let actor = spawn("echo", inbox, move || Ok(Echo { seen }))?;
    drop(sender);
    actor.join()?;
    Ok(())
}

#[test]
fn a_start_that_fails_returns_its_failure() {
    let (_sender, inbox) = mailbox();
    let started = spawn::<Echo, _>("echo", inbox, || Err(Failed));
    assert!(matches!(started, Err(SpawnError::Start(Failed))));
}

#[test]
fn join_returns_the_failure_of_the_actor() -> TestResult {
    let (sender, inbox) = mailbox();
    let (seen, _values) = mpsc::channel();
    let actor = spawn("echo", inbox, move || Ok(Echo { seen }))?;
    sender.push(Input::Fail)?;
    assert!(matches!(actor.join(), Err(JoinError::Failed(Failed))));
    Ok(())
}
