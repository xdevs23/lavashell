// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::view::{ComponentError, ComponentFailure, Failures, FrameError};

#[derive(Debug, thiserror::Error)]
pub enum FrameFailure {
    #[error("a frame of a view could not be made")]
    Frame(#[from] FrameError),
    #[error("a component of a view failed in a frame")]
    Components(Vec<ComponentFailure>),
}

pub trait WithoutFailures {
    fn without_failures(self) -> Result<(), FrameFailure>;
}

impl WithoutFailures for Result<Failures, FrameError> {
    fn without_failures(self) -> Result<(), FrameFailure> {
        self?.into_result().map_err(FrameFailure::Components)
    }
}

pub fn failure_causes<T>(
    result: Result<T, FrameFailure>,
) -> Result<Vec<ComponentError>, FrameFailure> {
    match result {
        Ok(_) => Ok(Vec::new()),
        Err(FrameFailure::Components(failures)) => {
            Ok(failures.into_iter().map(|failure| failure.cause).collect())
        }
        Err(error) => Err(error),
    }
}
