// SPDX-License-Identifier: GPL-3.0-only
use crate::TestPlatform;
use lavashell_core::frame::RenderStats;
use lavashell_core::geometry::{PixelSize, ScaledViewport};
use lavashell_core::scene::tree::SceneSnapshot;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::units::NumberError;
use lavashell_platform::{
    Attach, Backend, Drawn, FrameOutcome, HeadlessRendererBackend, PresentFrame, RendererBackend,
    RendererSetup, StreamTask, SurfaceContents, SurfaceControl, SurfaceRenderer,
};
use lavashell_renderer::Painter;
use lavashell_renderer::skia::{self, Path};
use lavashell_renderer::text::TextRendering;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Duration;

pub(crate) type TestPlatformWithoutDrawing = TestPlatform<TestBackend>;

pub(crate) struct TestBackend;

impl Backend for TestBackend {
    type Surface = ();
    type Streams = ();
}

impl HeadlessRendererBackend for TestBackend {
    fn create() -> Self {
        Self
    }

    fn create_surface() {}
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
        Ok(TestRenderer {
            painter: setup.painter_without_device()?,
            surfaces: BTreeMap::new(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestRendererError<E> {
    #[error("the surface {0:?} is attached already")]
    AttachedTwice(SurfaceId),
    #[error("the surface {0:?} is not attached")]
    UnknownSurface(SurfaceId),
    #[error("the frame of the surface {surface:?} has no buffer size")]
    BufferSize {
        surface: SurfaceId,
        #[source]
        source: NumberError,
    },
    #[error("cannot make a blank {size:?} canvas for the surface {surface:?}")]
    BlankCanvas { surface: SurfaceId, size: PixelSize },
    #[error("the control of a surface failed to take its painted frame")]
    Painted(#[source] E),
    #[error("the control of a surface failed to commit its state")]
    CommitState(#[source] E),
}

struct AttachedSurface<C> {
    control: C,
    drawn: Option<ScaledViewport>,
}

pub(crate) struct TestRenderer<C> {
    painter: Painter,
    surfaces: BTreeMap<SurfaceId, AttachedSurface<C>>,
}

impl<C: SurfaceControl> TestRenderer<C> {
    fn attached(
        &mut self,
        surface: SurfaceId,
    ) -> Result<&mut AttachedSurface<C>, TestRendererError<C::Error>> {
        (self.surfaces.get_mut(&surface)).ok_or(TestRendererError::UnknownSurface(surface))
    }
}

impl<C: SurfaceControl> SurfaceRenderer<C, (), ()> for TestRenderer<C> {
    type Error = TestRendererError<C::Error>;
    type DrawError = TestRendererError<C::Error>;
    type CommitError = TestRendererError<C::Error>;

    fn text_rendering(&self) -> TextRendering {
        self.painter.text_rendering()
    }

    fn attach(&mut self, attach: Attach<C, ()>) -> Result<(), TestRendererError<C::Error>> {
        let Entry::Vacant(entry) = self.surfaces.entry(attach.surface) else {
            return Err(TestRendererError::AttachedTwice(attach.surface));
        };
        let _attached_surface_is_not_needed = entry.insert(AttachedSurface {
            control: attach.control,
            drawn: None,
        });
        Ok(())
    }

    fn retire(&mut self, surface: SurfaceId) -> Option<C> {
        (self.surfaces.remove(&surface)).map(|attached| attached.control)
    }

    fn control_mut(&mut self, surface: SurfaceId) -> Option<&mut C> {
        (self.surfaces.get_mut(&surface)).map(|attached| &mut attached.control)
    }

    fn set_outline(
        &mut self,
        surface: SurfaceId,
        _: Option<Path>,
    ) -> Result<(), TestRendererError<C::Error>> {
        self.attached(surface).map(drop)
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
    ) -> Result<(), TestRendererError<C::Error>> {
        Ok(())
    }

    fn draw(
        &mut self,
        surface: SurfaceId,
        frame: PresentFrame<'_>,
        _: &SceneSnapshot,
    ) -> Result<Drawn, TestRendererError<C::Error>> {
        let mut canvas = create_blank_canvas(surface, frame.scaled_viewport)?;
        let attached = self.attached(surface)?;
        (attached.control.painted(&frame, canvas.canvas())).map_err(TestRendererError::Painted)?;
        attached.control.after_present(FrameOutcome::Shown);
        attached.drawn = Some(frame.scaled_viewport);
        Ok(Drawn {
            outcome: FrameOutcome::Shown,
            stats: RenderStats::default(),
            gpu: None,
            output: None,
            scene_generation: None,
        })
    }

    fn contents(
        &self,
        surface: SurfaceId,
        viewport: ScaledViewport,
    ) -> Result<SurfaceContents, TestRendererError<C::Error>> {
        let drawn = (self.surfaces.get(&surface)).and_then(|attached| attached.drawn);
        Ok(if drawn == Some(viewport) {
            SurfaceContents::Current
        } else {
            SurfaceContents::Outdated
        })
    }

    fn commit_state(
        &mut self,
        surface: SurfaceId,
        state: &SurfaceState,
    ) -> Result<(), TestRendererError<C::Error>> {
        (self.attached(surface)?.control.commit_state(state))
            .map_err(TestRendererError::CommitState)
    }
}

fn create_blank_canvas<E>(
    surface: SurfaceId,
    viewport: ScaledViewport,
) -> Result<skia::Surface, TestRendererError<E>> {
    let size = (viewport.buffer_size())
        .map_err(|source| TestRendererError::BufferSize { surface, source })?;
    (size.signed())
        .and_then(skia::surfaces::raster_n32_premul)
        .ok_or(TestRendererError::BlankCanvas { surface, size })
}
