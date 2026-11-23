// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::ScaledViewport;
use lavashell_core::scene::tree::SceneSnapshot;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_platform::{
    Attach, Backend, Drawn, HeadlessRendererBackend, PresentFrame, RendererBackend, RendererSetup,
    StreamTask, SurfaceContents, SurfaceControl, SurfaceRenderer,
};
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use std::convert::Infallible;
use std::time::Duration;

pub enum NoBackend {}

impl HeadlessRendererBackend for NoBackend {
    fn create() -> Option<Self> {
        None
    }

    fn create_surface() {}
}

impl Backend for NoBackend {
    type Surface = ();
    type Streams = ();
}

impl RendererBackend for NoBackend {
    type Renderer<C: SurfaceControl> = Self;
    type StartError = Infallible;

    fn shows_scene(&self) -> bool {
        match *self {}
    }

    fn start<C: SurfaceControl>(self, _: RendererSetup) -> Result<Self, Infallible> {
        match self {}
    }
}

impl<C: SurfaceControl> SurfaceRenderer<C, (), ()> for NoBackend {
    type Error = Infallible;
    type DrawError = Infallible;
    type CommitError = Infallible;

    fn text_rendering(&self) -> TextRendering {
        match *self {}
    }

    fn attach(&mut self, _: Attach<C, ()>) -> Result<(), Infallible> {
        match *self {}
    }

    fn retire(&mut self, _: SurfaceId) -> Option<C> {
        match *self {}
    }

    fn control_mut(&mut self, _: SurfaceId) -> Option<&mut C> {
        match *self {}
    }

    fn set_outline(&mut self, _: SurfaceId, _: Option<Path>) -> Result<(), Infallible> {
        match *self {}
    }

    fn run_on_streams(&mut self, _: StreamTask<()>) {
        match *self {}
    }

    fn scene_deadline(&self) -> Option<Duration> {
        match *self {}
    }

    fn advance_scene(&mut self, _: ScaledViewport, _: Duration) -> Result<(), Infallible> {
        match *self {}
    }

    fn draw(
        &mut self,
        _: SurfaceId,
        _: PresentFrame<'_>,
        _: &SceneSnapshot,
    ) -> Result<Drawn, Infallible> {
        match *self {}
    }

    fn contents(&self, _: SurfaceId, _: ScaledViewport) -> Result<SurfaceContents, Infallible> {
        match *self {}
    }

    fn commit_state(&mut self, _: SurfaceId, _: &SurfaceState) -> Result<(), Infallible> {
        match *self {}
    }
}
