// SPDX-License-Identifier: GPL-3.0-only
use lavashell::keyboard::LayoutError;
use lavashell::prelude::AnimationError;
use lavashell_example_support::{ExampleError, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("the keyboard of the scene cannot be made")]
    Keyboard(#[source] AnimationError),
    #[error("the layouts shipped with the keyboard cannot be built")]
    Layouts(#[from] LayoutError),
}

impl ExampleError for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Setup(_) | Self::Keyboard(_) | Self::Layouts(_) => NextStep::ReportDefect,
        }
    }
}
