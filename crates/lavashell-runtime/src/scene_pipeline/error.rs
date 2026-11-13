// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{JoinError, SpawnError};
use lavashell_core::scene::tree::ApplyError;
use std::convert::Infallible;

/// The failures that can occur while the choreographer and the scene builder start and run.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the choreographer could not start")]
    StartChoreographer(#[source] SpawnError<Infallible>),
    #[error("the scene builder could not start")]
    StartBuilder(#[source] SpawnError<ApplyError>),
    #[error("the choreographer failed")]
    RunChoreographer(#[source] JoinError<Infallible>),
    #[error("the scene builder failed")]
    RunBuilder(#[source] JoinError<ApplyError>),
}
