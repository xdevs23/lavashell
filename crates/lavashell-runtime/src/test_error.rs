// SPDX-License-Identifier: GPL-3.0-only
//! The failures that the tests of this crate return in place of a panic.

use crate::run::RunError;
use crate::test_host::StandInError;
use crate::view_actor::{StartError, ViewError};
use lavashell_actor::Closed;
use lavashell_core::units::NumberError;
use lavashell_core::view::FrameError;
use lavashell_test_fixtures::FrameFailure;
use rustix::io::Errno;
use std::convert::Infallible;
use std::sync::mpsc::RecvError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("a view could not frame")]
    Frame(#[from] FrameError),
    #[error("a frame of a view failed")]
    FrameFailure(#[from] FrameFailure),
    #[error("a number is not valid")]
    Number(#[from] NumberError),
    #[error("a mailbox of the test closed")]
    Closed(#[from] Closed),
    #[error("the run failed")]
    Run(#[from] RunError<StandInError, Infallible>),
    #[error("a view failed")]
    View(#[from] ViewError),
    #[error("the views could not start")]
    Start(#[from] StartError<Infallible>),
    #[error("the scripted platform did not hand over its requests")]
    NoRequests(#[from] RecvError),
    #[error("a file descriptor could not be created")]
    Descriptor(#[from] Errno),
    #[error(transparent)]
    AccessibilityTest(#[from] crate::accessibility::test_error::AccessibilityTestError),
    #[error(transparent)]
    FailuresTest(#[from] crate::failures::test_error::FailuresTestError),
    #[error(transparent)]
    PanelsTest(#[from] crate::panels::test_error::PanelsTestError),
    #[error(transparent)]
    SurfaceRecordsTest(#[from] crate::surface_records::test_error::SurfaceRecordsTestError),
    #[error(transparent)]
    Watches(#[from] crate::watches::Error),
}

pub(crate) type TestResult<T = ()> = Result<T, TestError>;
