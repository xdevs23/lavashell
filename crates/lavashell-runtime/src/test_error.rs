// SPDX-License-Identifier: GPL-3.0-only
//! The failures returned by the tests of this crate instead of a panic.

use crate::Error;
use lavashell_core::units::NumberError;
use lavashell_core::view::FrameError;
use lavashell_test_fixtures::FrameFailure;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("the runtime failed")]
    Runtime(#[from] Error),
    #[error("a view could not frame")]
    Frame(#[from] FrameError),
    #[error("a frame of a view failed")]
    FrameFailure(#[from] FrameFailure),
    #[error("a number is not valid")]
    Number(#[from] NumberError),
    #[error("the surface is not open")]
    NotOpen,
    #[error("the session lock offered no view for the output")]
    NoLockView,
    #[error("the surface has no panel")]
    NoPanel,
}

pub(crate) type TestResult = Result<(), TestError>;
