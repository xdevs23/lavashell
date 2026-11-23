// SPDX-License-Identifier: GPL-3.0-only
use lavashell_enum_values::enum_with_values;
use std::convert::Infallible;
use std::fmt;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NextStep {
        FreeSystemResources,
        FreeGpuMemory,
        UpdateGraphicsDriver,
        StartInDesktopSession,
        UseSupportingPlatform,
        ReplaceTheFontFile,
        ChangeTheScene,
        ChangeTheStreamProducer,
        RepeatTheTransfer,
        ReportTheFailure,
    }

    pub const ALL;
}

impl fmt::Display for NextStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FreeSystemResources => {
                "Close programs you do not need to free file descriptors and memory, then start \
                 the app again."
            }
            Self::FreeGpuMemory => {
                "Close programs you do not need to free GPU memory, or make the window smaller."
            }
            Self::UpdateGraphicsDriver => {
                "Install or update the graphics driver, then start the app again."
            }
            Self::StartInDesktopSession => "Start the app again inside a running desktop session.",
            Self::UseSupportingPlatform => {
                "Start the app on a platform that supports everything the app uses, or remove the \
                 parts of the app that the platform does not support."
            }
            Self::ReplaceTheFontFile => {
                "Replace the font file with a TrueType or OpenType font file that can be read and \
                 decoded, then start the app again."
            }
            Self::ChangeTheScene => {
                "Change the scene of the app so that it no longer draws what this message \
                 describes."
            }
            Self::ChangeTheStreamProducer => {
                "Change the program that draws into the stream so that its frames avoid what this \
                 message describes."
            }
            Self::RepeatTheTransfer => "Copy, paste or drag the data again.",
            Self::ReportTheFailure => "Report the failure with this log.",
        })
    }
}

pub trait NamesNextStep {
    fn next_step(&self) -> NextStep;
}

impl NamesNextStep for Infallible {
    fn next_step(&self) -> NextStep {
        match *self {}
    }
}

impl<E: NamesNextStep> NamesNextStep for [E] {
    fn next_step(&self) -> NextStep {
        (self.iter().map(E::next_step))
            .reduce(|chosen, step| match step {
                NextStep::ReportTheFailure => step,
                _ => chosen,
            })
            .unwrap_or(NextStep::ReportTheFailure)
    }
}

#[cfg(test)]
mod tests;
