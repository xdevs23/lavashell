// SPDX-License-Identifier: GPL-3.0-only
use lavashell::core::animation::{AnimationError, ContourError, KeyframesError};
use lavashell::error_chain::next_step::NextStep;
use lavashell::prelude::NumberError;
use lavashell_example_support::{ExampleError, ExampleStep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Example(#[from] lavashell_example_support::Error),
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error("cannot sample a contour of the morph")]
    Contour(#[source] ContourError),
    #[error("cannot count the samples of the morph")]
    MorphSamples(#[source] NumberError),
    #[error("the keys of the timeline are invalid")]
    Timeline(#[source] KeyframesError),
    #[error("a motion of the demo cannot take its value")]
    Motion(#[source] AnimationError),
}

impl ExampleError for Error {
    fn next_step(&self) -> ExampleStep {
        match self {
            Self::Example(error) => error.next_step(),
            Self::Setup(_)
            | Self::Contour(_)
            | Self::MorphSamples(_)
            | Self::Timeline(_)
            | Self::Motion(_) => NextStep::ReportTheFailure.into(),
        }
    }
}
