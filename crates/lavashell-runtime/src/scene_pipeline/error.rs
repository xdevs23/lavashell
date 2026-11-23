// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{JoinError, SpawnError};
use lavashell_core::scene::tree::ApplyError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use std::convert::Infallible;

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

impl NamesNextStep for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::StartChoreographer(SpawnError::Thread(_))
            | Self::StartBuilder(SpawnError::Thread(_)) => NextStep::FreeSystemResources,
            Self::StartChoreographer(SpawnError::Start(_) | SpawnError::Panicked)
            | Self::StartBuilder(SpawnError::Start(_) | SpawnError::Panicked)
            | Self::RunChoreographer(_)
            | Self::RunBuilder(_) => NextStep::ReportTheFailure,
        }
    }
}
