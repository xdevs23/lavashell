// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::animation::{AnimationError, KeyframesError};
use lavashell_example_support::{ExampleError, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("the keys of the phase loop are invalid")]
    PhaseLoop(#[source] KeyframesError),
    #[error("the motion of the phase loop cannot be made")]
    PhaseMotion(#[source] AnimationError),
}

impl ExampleError for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Setup(_) | Self::PhaseLoop(_) | Self::PhaseMotion(_) => NextStep::ReportDefect,
        }
    }
}
