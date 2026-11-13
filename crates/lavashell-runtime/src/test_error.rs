// SPDX-License-Identifier: GPL-3.0-only
//! The failures returned by the tests of this crate instead of a panic.

use crate::test_host::StandInError;
use crate::view_actor::StartError;
use crate::{Error, RunError, ViewError};
use accesskit::{NodeId as NativeId, Role as NativeRole};
use lavashell_actor::Closed;
use lavashell_core::prelude::NodeId;
use lavashell_core::units::NumberError;
use lavashell_core::view::FrameError;
use lavashell_test_fixtures::FrameFailure;
use log::SetLoggerError;
use rustix::io::Errno;
use std::convert::Infallible;
use std::sync::mpsc::RecvError;

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
    #[error("the platform could not show a frame")]
    NotShown,
    #[error("the runtime published no tree for the surface")]
    NotPublished,
    #[error("the published tree has no button")]
    NoButton,
    #[error("the tree has no node {0:?}")]
    MissingNode(NodeId),
    #[error("the node {0:?} has no text")]
    MissingText(NodeId),
    #[error("the native tree has no node with the role {0:?}")]
    MissingNativeRole(NativeRole),
    #[error("the native tree has no node {0:?}")]
    MissingNativeNode(NativeId),
    #[error("the native tree has no tree information")]
    MissingTreeInfo,
    #[error("the native node has no text selection")]
    MissingSelection,
    #[error("the action request maps to no action of Lavashell")]
    RefusedAction,
    #[error("the native text has no position at index {0}")]
    MissingPosition(usize),
    #[error("the native text has no line {0}")]
    MissingLine(usize),
    #[error("the test process has a logger that is not the recorder of the test")]
    Logger(#[source] &'static SetLoggerError),
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
}

pub(crate) type TestResult = Result<(), TestError>;
