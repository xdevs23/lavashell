// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, FONT_DIRECTORY};
use std::fmt;

pub enum NextStep {
    EnterDevelopmentShell,
    ProvideVulkanDriver,
    StartInWaylandSession,
    CorrectOptions,
    ChooseWritableCapture,
    ChooseWritableFrameLog,
    SetHome,
    MakeCacheWritable,
    ReportDefect,
}

pub trait ExampleError: std::error::Error {
    fn next_step(&self) -> NextStep;
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnterDevelopmentShell => write!(
                formatter,
                "Run the example with `nix run` or inside `nix develop`, which set {FONT_DIRECTORY}."
            ),
            Self::ProvideVulkanDriver => {
                formatter.write_str("Install a Vulkan driver, then run the example again.")
            }
            Self::StartInWaylandSession => {
                formatter.write_str("Start the example inside a Wayland session.")
            }
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
            Self::ReportDefect => formatter.write_str(
                "The example is defective. Report this output to the developers of the project.",
            ),
        }
    }
}

impl ExampleError for Error {
    fn next_step(&self) -> NextStep {
        match self {
            Self::FontDirectory | Self::Fonts(_) => NextStep::EnterDevelopmentShell,
            Self::App(_) => NextStep::StartInWaylandSession,
            Self::Number(_) | Self::TextScale | Self::Seconds(_) | Self::Length(_) => {
                NextStep::CorrectOptions
            }
            Self::CreateFrameLog(_) | Self::WriteFrameReport(_) | Self::FlushFrameLog(_) => {
                NextStep::ChooseWritableFrameLog
            }
            Self::Shaper(_)
            | Self::Frame(_)
            | Self::ComponentsFailed(_)
            | Self::MissingControl(_)
            | Self::MissingNode { .. }
            | Self::Setup(_)
            | Self::Icon(_) => NextStep::ReportDefect,
        }
    }
}
