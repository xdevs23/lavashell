// SPDX-License-Identifier: GPL-3.0-only
use lavashell_actor::{JoinError, SpawnError};
use std::convert::Infallible;

/// The failures that can occur while the frame scheduler starts and runs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the frame scheduler could not start")]
    StartScheduler(#[source] SpawnError<Infallible>),
    #[error("the frame scheduler failed")]
    RunScheduler(#[source] JoinError<Infallible>),
}
