// SPDX-License-Identifier: GPL-3.0-only
use crate::model::App;
use lavashell_example_support::{ExampleError, NextStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("the scene has no taskbar")]
    MissingBar,
    #[error("the taskbar has no slot for {0:?}")]
    MissingSlot(App),
}

impl ExampleError for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Setup(_) | Self::MissingBar | Self::MissingSlot(_) => NextStep::ReportDefect,
        }
    }
}
