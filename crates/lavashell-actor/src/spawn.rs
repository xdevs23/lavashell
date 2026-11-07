// SPDX-License-Identifier: GPL-3.0-only
use crate::{Actor, Flow, Merge, Receiver};
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

/// Starts a thread called `name`. The thread builds the actor with `start` and passes each input from `inbox` to the
/// actor until the actor stops or fails, or every sender of `inbox` is gone. Returns after `start` has returned, so
/// the actor is built on the thread that runs it.
///
/// # Errors
///
/// Fails when the thread cannot be created, with the error returned by `start`, or when the thread ends inside
/// `start`.
pub fn spawn<A, F>(
    name: &str,
    inbox: Receiver<A::Input>,
    start: F,
) -> Result<Handle<A>, SpawnError<A::Error>>
where
    A: Actor,
    <A::Input as Merge>::Key: Send,
    F: FnOnce() -> Result<A, A::Error> + Send + 'static,
{
    let (started, start_result) = mpsc::sync_channel(1);
    let thread = (thread::Builder::new().name(name.to_owned()))
        .spawn(move || {
            let mut actor = start()?;
            let _announced = started.send(());
            run(&mut actor, &inbox)
        })
        .map_err(SpawnError::Thread)?;
    if start_result.recv().is_ok() {
        return Ok(Handle { thread });
    }
    match thread.join() {
        Ok(Err(error)) => Err(SpawnError::Start(error)),
        Ok(Ok(())) | Err(_) => Err(SpawnError::Ended),
    }
}

fn run<A: Actor>(actor: &mut A, inbox: &Receiver<A::Input>) -> Result<(), A::Error> {
    while let Ok(inputs) = inbox.take() {
        for input in inputs {
            if actor.handle(input)? == Flow::Stop {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// The thread of a running actor.
pub struct Handle<A: Actor> {
    thread: JoinHandle<Result<(), A::Error>>,
}

impl<A: Actor> Handle<A> {
    /// Waits until the actor stopped.
    ///
    /// # Errors
    ///
    /// Fails with the error returned by the actor, or when the thread running the actor panicked.
    pub fn join(self) -> Result<(), JoinError<A::Error>> {
        match self.thread.join() {
            Ok(stopped) => stopped.map_err(JoinError::Failed),
            Err(_) => Err(JoinError::Panicked),
        }
    }
}

/// The failure to start an actor.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError<E> {
    #[error("the thread for the actor could not be created")]
    Thread(#[source] io::Error),
    #[error("the actor could not start")]
    Start(#[source] E),
    #[error("the thread for the actor ended before the actor started")]
    Ended,
}

/// The failure of an actor that stopped.
#[derive(Debug, thiserror::Error)]
pub enum JoinError<E> {
    #[error("the actor failed")]
    Failed(#[source] E),
    #[error("the thread running the actor panicked")]
    Panicked,
}

#[cfg(test)]
mod tests;
