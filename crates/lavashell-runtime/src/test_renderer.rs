// SPDX-License-Identifier: GPL-3.0-only

use crate::controls_actor::{ControlsError, SurfaceControlTable};
use lavashell_core::frame::RenderStats;
use lavashell_core::geometry::ScaledViewport;
use lavashell_core::scene::tree::SceneSnapshot;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_platform::{
    Attach, Backend, Drawn, FrameOutcome, PresentFrame, RendererBackend, RendererSetup, StreamTask,
    SurfaceContents, SurfaceControl, SurfaceRenderer,
};
use lavashell_renderer::Painter;
use lavashell_renderer::skia::Path;
use lavashell_renderer::text::TextRendering;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct TestBackend {
    start_notice: Option<mpsc::Sender<()>>,
}

impl TestBackend {
    pub(crate) fn noticing_start(start_notice: mpsc::Sender<()>) -> Self {
        Self {
            start_notice: Some(start_notice),
        }
    }
}

impl Backend for TestBackend {
    type Surface = ();
    type Streams = ();
}

impl RendererBackend for TestBackend {
    type Renderer<C: SurfaceControl> = TestRenderer<C>;
    type StartError = lavashell_renderer::Error;

    fn shows_scene(&self) -> bool {
        false
    }

    fn start<C: SurfaceControl>(
        self,
        setup: RendererSetup,
    ) -> Result<TestRenderer<C>, lavashell_renderer::Error> {
        let painter = setup.painter_without_device()?;
        if let Some(notice) = self.start_notice {
            let _test_gone = notice.send(());
        }
        Ok(TestRenderer {
            painter,
            controls: SurfaceControlTable::default(),
            drawn: BTreeMap::new(),
        })
    }
}

pub(crate) struct TestRenderer<C> {
    painter: Painter,
    controls: SurfaceControlTable<C>,
    drawn: BTreeMap<SurfaceId, ScaledViewport>,
}

impl<C: SurfaceControl> SurfaceRenderer<C, (), ()> for TestRenderer<C> {
    type Error = ControlsError<C::Error>;
    type DrawError = ControlsError<C::Error>;
    type CommitError = ControlsError<C::Error>;

    fn text_rendering(&self) -> TextRendering {
        self.painter.text_rendering()
    }

    fn attach(&mut self, attach: Attach<C, ()>) -> Result<(), ControlsError<C::Error>> {
        self.controls.attach(attach.surface, attach.control)
    }

    fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        let _last_drawn_viewport = self.drawn.remove(&surface);
        self.controls.retire(surface)
    }

    fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C> {
        self.controls.attached_control(surface).ok()
    }

    fn set_outline(
        &mut self,
        surface: SurfaceId,
        _: Option<Path>,
    ) -> Result<(), ControlsError<C::Error>> {
        self.controls.attached_control(surface).map(|_| ())
    }

    fn run_on_streams(&mut self, task: StreamTask<()>) {
        task(&mut (), &self.painter);
    }

    fn scene_deadline(&self) -> Option<Duration> {
        None
    }

    fn advance_scene(
        &mut self,
        _: ScaledViewport,
        _: Duration,
    ) -> Result<(), ControlsError<C::Error>> {
        Ok(())
    }

    fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        _: &SceneSnapshot,
    ) -> Result<Drawn, ControlsError<C::Error>> {
        self.controls.attached_control(surface).map(|_| {
            let _earlier_viewport = self.drawn.insert(surface, frame.scaled_viewport);
            Drawn {
                outcome: FrameOutcome::Shown,
                stats: RenderStats::default(),
                gpu: None,
                output: None,
                scene_generation: None,
            }
        })
    }

    fn contents(
        &self,
        surface: SurfaceId,
        viewport: ScaledViewport,
    ) -> Result<SurfaceContents, ControlsError<C::Error>> {
        Ok(if self.drawn.get(&surface) == Some(&viewport) {
            SurfaceContents::Current
        } else {
            SurfaceContents::Outdated
        })
    }

    fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), ControlsError<C::Error>> {
        self.controls.commit_state(surface, state)
    }
}
