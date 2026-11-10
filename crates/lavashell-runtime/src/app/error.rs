// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;
use lavashell_core::units::NumberError;

/// An app setup that cannot run. Each variant identifies the wrong part.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("the app has a session lock already")]
    SessionLockConfigured,
    #[error("the app has a per-output layer factory already")]
    OutputLayersConfigured,
    #[error("the window size {size:?} is not finite or is not above zero")]
    WindowSize {
        size: Size,
        #[source]
        source: NumberError,
    },
    #[error("the minimum window size {size:?} is not finite or is below zero")]
    MinimumWindowSize {
        size: Size,
        #[source]
        source: NumberError,
    },
}
