// SPDX-License-Identifier: GPL-3.0-only
use crate::renderer_actor::RendererStopped;
use crate::shape_geometry::ShapeGeometryError;
use lavashell_actor::Closed;
use lavashell_core::shape::PointsError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::FrameError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Stopped {
    #[error("the scene builder stopped")]
    Scene,
    #[error("the renderer stopped")]
    Renderer,
    #[error("the surface controls kept without a renderer stopped")]
    Controls,
    #[error("the frame scheduler stopped")]
    Scheduler,
    #[error("the platform stopped")]
    Platform,
}

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

impl NamesNextStep for ViewError {
    fn next_step(&self) -> NextStep {
        match self {
            Self::ScenePipeline(error) => error.next_step(),
            Self::DeadlineReceiver(_)
            | Self::FocusGrab(_)
            | Self::Panels(_)
            | Self::SchedulerActor(_)
            | Self::SessionLock(_)
            | Self::SurfaceRecords(_)
            | Self::ShowScene(_)
            | Self::Stopped(_)
            | Self::ShapeContour(_)
            | Self::ShapeGeometry(_)
            | Self::SurfaceNotOpen(_) => NextStep::ReportTheFailure,
        }
    }
}

impl From<RendererStopped> for ViewError {
    fn from(RendererStopped: RendererStopped) -> Self {
        Self::Stopped(Stopped::Renderer)
    }
}
