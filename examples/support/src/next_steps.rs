// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, FONT_DIRECTORY};
use lavashell::error_chain::next_step::{NamesNextStep, NextStep};
use std::fmt;

pub enum ExampleStep {
    EnterDevelopmentShell,
    CorrectOptions,
    ChooseWritableCapture,
    ChooseWritableFrameLog,
    SetHome,
    MakeCacheWritable,
    Shared(NextStep),
}

pub trait ExampleError: std::error::Error {
    fn next_step(&self) -> ExampleStep;
}

impl<E: std::error::Error + NamesNextStep> ExampleError for E {
    fn next_step(&self) -> ExampleStep {
        NamesNextStep::next_step(self).into()
    }
}

impl From<NextStep> for ExampleStep {
    fn from(step: NextStep) -> Self {
        Self::Shared(step)
    }
}

impl fmt::Display for ExampleStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnterDevelopmentShell => write!(
                formatter,
                "Run the example with `nix run` or inside `nix develop`, which set {FONT_DIRECTORY}."
            ),
            Self::CorrectOptions => {
                formatter.write_str("Correct the options of the example, then run it again.")
            }
            Self::ChooseWritableCapture => formatter.write_str(
                "Choose a capture path in a writable directory with free space, then run the \
                 example again.",
            ),
            Self::ChooseWritableFrameLog => formatter.write_str(
                "Set LAVASHELL_FRAME_LOG to a file in a writable directory with free space, then \
                 run the example again.",
            ),
            Self::SetHome => {
                formatter.write_str("Set HOME to a home directory, then run the example again.")
            }
            Self::MakeCacheWritable => formatter.write_str(
                "Make the cache directory in the message above writable and give it free space, \
                 then run the example again.",
            ),
            Self::Shared(step) => fmt::Display::fmt(step, formatter),
        }
    }
}

impl ExampleError for Error {
    fn next_step(&self) -> ExampleStep {
        match self {
            Self::FontDirectory | Self::Fonts(_) => ExampleStep::EnterDevelopmentShell,
            Self::App(error) => ExampleError::next_step(error),
            Self::Shaper(error) => ExampleError::next_step(error),
            Self::Number(_) | Self::TextScale | Self::Seconds(_) | Self::Length(_) => {
                ExampleStep::CorrectOptions
            }
            Self::CreateFrameLog(_) | Self::WriteFrameReport(_) | Self::FlushFrameLog(_) => {
                ExampleStep::ChooseWritableFrameLog
            }
            Self::Frame(_)
            | Self::ComponentsFailed(_)
            | Self::MissingControl(_)
            | Self::MissingNode { .. }
            | Self::Setup(_)
            | Self::Icon(_) => NextStep::ReportTheFailure.into(),
        }
    }
}
