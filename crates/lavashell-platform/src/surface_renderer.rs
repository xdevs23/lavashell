// SPDX-License-Identifier: GPL-3.0-only
use crate::{Attach, FrameOutcome, PresentFrame, StreamTask, SurfaceControl};
use lavashell_core::frame::{GpuFrameTime, RenderStats};
use lavashell_core::geometry::ScaledViewport;
use lavashell_core::output::OutputInfo;
use lavashell_core::scene::tree::SceneSnapshot;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use std::time::Duration;

pub trait SurfaceRenderer<C: SurfaceControl, Surface, Streams> {
    type Error: std::error::Error + Send + 'static;
    type DrawError: std::error::Error + Send + 'static;
    type CommitError: std::error::Error + Send + 'static;

    fn text_rendering(&self) -> TextRendering;

    fn attach(&mut self, attach: Attach<C, Surface>) -> Result<(), Self::Error>;

    fn retire(&mut self, surface: SurfaceId) -> Option<C>;

    fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C>;

    fn set_outline(&mut self, surface: SurfaceId, outline: Option<Path>)
    -> Result<(), Self::Error>;

    fn run_on_streams(&mut self, task: StreamTask<Streams>);

    fn scene_deadline(&self) -> Option<Duration>;

    fn advance_scene(
        &mut self,
        scaled_viewport: ScaledViewport,
        now: Duration,
    ) -> Result<(), Self::Error>;

    fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        snapshot: &SceneSnapshot,
    ) -> Result<Drawn, Self::DrawError>;

    fn contents(
        &self,
        surface: SurfaceId,
        scaled_viewport: ScaledViewport,
    ) -> Result<SurfaceContents, Self::Error>;

    fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), Self::CommitError>;
}

#[derive(Clone, Copy, Debug)]
pub struct Drawn {
    pub outcome: FrameOutcome,
    pub stats: RenderStats,
    pub gpu: Option<GpuFrameTime>,
    pub output: Option<OutputInfo>,
    pub scene_generation: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceContents {
    Current,
    Outdated,
}
