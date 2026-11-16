// SPDX-License-Identifier: GPL-3.0-only
use crate::{Actor, Flow, Merge, Receiver};
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

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
    let (started, start_outcome) = mpsc::sync_channel(1);
    let thread = (thread::Builder::new().name(name.to_owned()))
        .spawn(move || {
            let mut actor = match start() {
                Ok(actor) => actor,
                Err(error) => {
                    let _spawn_waits_for_the_outcome = started.send(Err(error));
                    return Ok(());
                }
            };
            let _spawn_waits_for_the_outcome = started.send(Ok(()));
            run(&mut actor, &inbox)
        })
        .map_err(SpawnError::Thread)?;
    match start_outcome.recv() {
        Ok(Ok(())) => Ok(Handle { thread }),
        Ok(Err(error)) => Err(SpawnError::Start(error)),
        Err(mpsc::RecvError) => Err(SpawnError::Panicked),
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

pub struct Handle<A: Actor> {
    thread: JoinHandle<Result<(), A::Error>>,
}

impl<A: Actor> Handle<A> {
    pub fn join(self) -> Result<(), JoinError<A::Error>> {
        match self.thread.join() {
            Ok(stopped) => stopped.map_err(JoinError::Failed),
            Err(_) => Err(JoinError::Panicked),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError<E> {
    #[error("the thread for the actor could not be created")]
    Thread(#[source] io::Error),
    #[error("the actor could not start")]
    Start(#[source] E),
    #[error("the thread for the actor panicked before the actor started")]
    Panicked,
}

#[derive(Debug, thiserror::Error)]
pub enum JoinError<E> {
    #[error("the actor failed")]
    Failed(#[source] E),
    #[error("the thread running the actor panicked")]
    Panicked,
}

#[cfg(test)]
mod tests;
