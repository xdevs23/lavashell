// SPDX-License-Identifier: GPL-3.0-only

use crate::renderer_actor::{RendererError, RendererStopped};
use crate::run::RunError;
use crate::scene_pipeline::BuilderReport;
use crate::test_host::{NoControl, StandIn};
use crate::view_actor::ViewError;
use lavashell_actor::{Closed, JoinError, SpawnError};
use lavashell_core::transfer::TransferError;
use lavashell_core::units::NumberError;
use lavashell_core::view::FrameError;
use lavashell_test_fixtures::FrameFailure;
use lavashell_vulkan_renderer::NoStreams;
use rustix::io::Errno;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::RecvError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestError {
    #[error("a view could not frame")]
    Frame(#[from] FrameError),
    #[error("a view failed to send a frame to the scene builder")]
    SendFrame(#[from] FrameError<Closed>),
    #[error("a frame of a view failed")]
    FrameFailure(#[from] FrameFailure),
    #[error("a number is not valid")]
    Number(#[from] NumberError),
    #[error("a mailbox of the test closed")]
    Closed(#[from] Closed),
    #[error("the run failed")]
    Run(#[from] RunError<StandIn>),
    #[error("the app is not set up to run")]
    Setup(#[from] crate::app::SetupError),
    #[error("a view failed")]
    View(#[from] ViewError),
    #[error("a channel of the test closed before it sent a value")]
    Receive(#[from] RecvError),
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
    #[error("the clipboard failed")]
    Clipboard(#[from] crate::clipboard::Error),
    #[error("the surface records failed")]
    Records(#[from] crate::surface_records::Error),
    #[error("the transfer data is not valid")]
    Transfer(#[from] TransferError),
    #[error("a pipe or socket of the test failed")]
    Pipe(#[from] io::Error),
    #[error("the deadline receiver failed")]
    DeadlineReceiver(#[from] crate::deadline_receiver::Error),
    #[error("the session lock failed")]
    SessionLock(#[from] crate::session_lock::Error),
    #[error("a transfer through a pipe failed")]
    PipeTransfer(#[from] crate::pipe_transfers::Error),
    #[error("the scheduler could not start or stop")]
    SchedulerActor(#[from] crate::scheduler_actor::Error),
    #[error("the renderer could not start")]
    RendererStart(#[from] SpawnError<RendererError<NoControl, NoStreams>>),
    #[error("the renderer failed")]
    RendererJoin(#[from] JoinError<RendererError<NoControl, NoStreams>>),
    #[error("the renderer stopped")]
    RendererStopped(#[from] RendererStopped),
    #[error("the scene pipeline failed to start")]
    ScenePipeline(#[from] crate::scene_pipeline::Error),
    #[error("the scene pipeline stopped with a failure")]
    SceneStopped(#[from] Arc<crate::scene_pipeline::Error>),
    #[error("the scene builder sent another report: {0:?}")]
    UnexpectedReport(BuilderReport),
    #[error("{0} is missing")]
    Missing(&'static str),
}

pub(crate) type TestResult<T = ()> = Result<T, TestError>;
