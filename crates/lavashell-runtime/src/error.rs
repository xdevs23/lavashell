// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{JoinError, SpawnError};
use lavashell_core::scene::tree::ApplyError;
use lavashell_core::units::NumberError;
use std::convert::Infallible;

/// A failure of the runtime, named after the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("every surface id is used")]
    NumberSurface,
    #[error("a new watch has a number too large for a watch id")]
    NumberWatch,
    #[error("the content of a surface could not be measured")]
    MeasureView(#[source] NumberError),
    #[error("the choreographer could not start")]
    StartChoreographer(#[source] SpawnError<Infallible>),
    #[error("the scene builder could not start")]
    StartBuilder(#[source] SpawnError<ApplyError>),
    #[error("the choreographer failed")]
    RunChoreographer(#[source] JoinError<Infallible>),
    #[error("the scene builder failed")]
    RunBuilder(#[source] JoinError<ApplyError>),
    #[error("the frame scheduler could not start")]
    StartScheduler(#[source] SpawnError<Infallible>),
    #[error("the frame scheduler failed")]
    RunScheduler(#[source] JoinError<Infallible>),
}
