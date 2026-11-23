// SPDX-License-Identifier: GPL-3.0-only
use lavashell::error_chain::next_step::NextStep;
use lavashell_example_support::{ExampleError, ExampleStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
}

impl ExampleError for Error {
    fn next_step(&self) -> ExampleStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Setup(_) => NextStep::ReportTheFailure.into(),
        }
    }
}
