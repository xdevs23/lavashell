// SPDX-License-Identifier: GPL-3.0-only
use crate::renderer_actor::{PlatformRendererError, RendererStopped};
use crate::run::RunError;
use crate::shape_geometry::ShapeGeometryError;
use lavashell_actor::{Closed, SpawnError};
use lavashell_core::shape::PointsError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::FrameError;
use lavashell_platform::Platform;
use std::fmt;

/// The actor or the platform thread that ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Stopped {
    #[error("the scene builder stopped")]
    Scene,
    #[error("the renderer stopped")]
    Renderer,
    #[error("the frame scheduler stopped")]
    Scheduler,
    #[error("the platform stopped")]
    Platform,
}

/// A failure of the views, or the stop of a part of the run that the views depend on.
#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error(transparent)]
    DeadlineReceiver(#[from] crate::deadline_receiver::Error),
    #[error(transparent)]
    FocusGrab(#[from] crate::focus_grab::Error),
    #[error(transparent)]
    Panels(#[from] crate::panels::Error),
    #[error(transparent)]
    ScenePipeline(#[from] crate::scene_pipeline::Error),
    #[error(transparent)]
    SchedulerActor(#[from] crate::scheduler_actor::Error),
    #[error(transparent)]
    SessionLock(#[from] crate::session_lock::Error),
    #[error(transparent)]
    SurfaceRecords(#[from] crate::surface_records::Error),
    #[error("a surface could not show a frame of its view")]
    ShowScene(#[source] FrameError<Closed>),
    #[error(transparent)]
    Stopped(#[from] Stopped),
    #[error("a contour of a window shape is not a valid polygon")]
    ShapeContour(#[from] PointsError),
    #[error(transparent)]
    ShapeGeometry(#[from] ShapeGeometryError),
    #[error("surface {0:?} is not open")]
    SurfaceNotOpen(SurfaceId),
}

impl From<RendererStopped> for ViewError {
    fn from(RendererStopped: RendererStopped) -> Self {
        Self::Stopped(Stopped::Renderer)
    }
}

/// A failure while the views start on the platform `P`.
#[derive(thiserror::Error)]
pub(crate) enum StartError<P: Platform> {
    #[error("the renderer could not start")]
    StartRenderer(#[source] SpawnError<PlatformRendererError<P>>),
    #[error("the shaper that measures text for the views could not be created")]
    CreateShaper(#[source] lavashell_renderer::Error),
    #[error(transparent)]
    View(#[from] ViewError),
}

impl<P: Platform> fmt::Debug for StartError<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, source): (&str, &dyn fmt::Debug) = match self {
            Self::StartRenderer(error) => ("StartRenderer", error),
            Self::CreateShaper(error) => ("CreateShaper", error),
            Self::View(error) => ("View", error),
        };
        formatter.debug_tuple(name).field(source).finish()
    }
}

impl<P: Platform> From<StartError<P>> for RunError<P> {
    fn from(error: StartError<P>) -> Self {
        match error {
            StartError::StartRenderer(error) => Self::StartRenderer(error),
            StartError::CreateShaper(error) => Self::CreateShaper(error),
            StartError::View(error) => Self::View(error),
        }
    }
}
