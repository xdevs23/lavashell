// SPDX-License-Identifier: GPL-3.0-only
use crate::ControlError;
use crate::Error;
use lavashell_core::geometry::Size;

/// A setup of an app that cannot run on Wayland. Each variant is one part of the setup that is wrong.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("the app has a session lock already")]
    SessionLockConfigured,
    #[error("the app has a per-output layer factory already")]
    OutputLayersConfigured,
    #[error("the window size {0:?} is not positive and finite or does not fit the protocol")]
    WindowSize(Size),
    #[error("the minimum window size {0:?} is not finite or does not fit the protocol")]
    MinimumWindowSize(Size),
    #[error("the app did not request any surface")]
    NoSurfaces,
}

/// A failure of a run of an app on Wayland.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("the app is not set up to run")]
    Setup(#[source] SetupError),
    #[error("the app failed while running")]
    Run(#[source] lavashell_runtime::run::RunError<Error, ControlError>),
}
