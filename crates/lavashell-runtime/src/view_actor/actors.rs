// SPDX-License-Identifier: GPL-3.0-only
use super::{Stopped, ViewError, ViewInput, observer};
use crate::controls_actor::{PlatformControlsError, SurfaceControls};
use crate::cpu_time::CpuTiming;
use crate::renderer_actor::{PlatformRendererError, Renderer, RendererInput, RendererStopped};
use crate::run::RunError;
use crate::scene_pipeline::{self, ScenePipeline};
use crate::scheduler::Redraw;
use crate::scheduler_actor::{self, SchedulerThread};
use crate::surface_slots::BuiltSurface;
use lavashell_actor::{Clock, Closed, JoinError, Outlet, Sender};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Platform, RendererSetup, Reports};
use lavashell_renderer::Painter;
use lavashell_renderer::text::TextRendering;
use std::sync::Arc;

pub(super) struct Actors<P: Platform> {
    pub(super) scene: ScenePipeline,
    pub(super) attached: AttachedSurfaces<P>,
    pub(super) scheduler: SchedulerThread,
}

pub(super) enum AttachedSurfaces<P: Platform> {
    Rendered(Renderer<P::Control, P::Backend>),
    Unrendered(SurfaceControls<P::Control>),
}

pub(crate) struct ActorStops<P: Platform> {
    pub(crate) renderer: Result<(), JoinError<PlatformRendererError<P>>>,
    pub(crate) controls: Result<(), JoinError<PlatformControlsError<P>>>,
    pub(crate) scheduler: Result<(), scheduler_actor::Error>,
    pub(crate) scene: Result<(), Arc<scene_pipeline::Error>>,
}

pub(super) struct ActorSetup<P: Platform> {
    pub(super) backend: Option<P::Backend>,
    pub(super) renderer: RendererSetup,
    pub(super) redraw: Redraw,
    pub(super) cpu_timing: CpuTiming,
}

impl<P: Platform> Actors<P> {
    pub(super) fn start(
        setup: ActorSetup<P>,
        clock: &Arc<dyn Clock>,
        inbox: &Sender<ViewInput>,
    ) -> Result<(Self, TextRendering), RunError<P>> {
        let ActorSetup {
            backend,
            renderer: renderer_setup,
            redraw,
            cpu_timing,
        } = setup;
        let builder = observer(inbox, ViewInput::Builder, Stopped::Scene);
        let scene = ScenePipeline::start(builder, clock.as_ref()).map_err(ViewError::from)?;
        let scheduler_outputs = observer(inbox, ViewInput::Scheduler, Stopped::Scheduler);
        let scheduler = SchedulerThread::start(redraw, clock.clone(), scheduler_outputs)
            .map_err(ViewError::from)?;
        let (attached, text_rendering) = if let Some(backend) = backend {
            let outputs = observer(inbox, ViewInput::Renderer, Stopped::Renderer);
            let renderer =
                Renderer::start(backend, renderer_setup, clock.clone(), cpu_timing, outputs)
                    .map_err(RunError::StartRenderer)?;
            let text_rendering = renderer.text_rendering();
            (AttachedSurfaces::Rendered(renderer), text_rendering)
        } else {
            let text_rendering = text_rendering_set_up_for_painter(renderer_setup)
                .map_err(RunError::CreatePainter)?;
            let outputs = observer(inbox, ViewInput::Controls, Stopped::Controls);
            let controls =
                SurfaceControls::start(clock.as_ref(), outputs).map_err(RunError::StartControls)?;
            (AttachedSurfaces::Unrendered(controls), text_rendering)
        };
        let actors = Self {
            scene,
            attached,
            scheduler,
        };
        Ok((actors, text_rendering))
    }

    pub(super) fn reports(
        &self,
        inbox: &Sender<ViewInput>,
        clock: Arc<dyn Clock>,
    ) -> Result<Reports<P::Control, P::Backend>, RendererStopped> {
        let (renderer, streams) = match &self.attached {
            AttachedSurfaces::Rendered(renderer) => {
                (renderer.attachments()?, renderer.stream_tasks()?)
            }
            AttachedSurfaces::Unrendered(controls) => (
                controls.attachments(),
                Outlet::new(|_task_without_streams| Ok(())),
            ),
        };
        Ok(Reports {
            view: inbox.outlet(ViewInput::Platform),
            scheduler: self.scheduler.timing_outlet(),
            renderer,
            streams,
            clock,
        })
    }

    pub(super) fn renderer(&self) -> Option<&Renderer<P::Control, P::Backend>> {
        match &self.attached {
            AttachedSurfaces::Rendered(renderer) => Some(renderer),
            AttachedSurfaces::Unrendered(_) => None,
        }
    }

    pub(super) fn commit_state(
        &self,
        surface: SurfaceId,
        built: Arc<BuiltSurface>,
    ) -> Result<(), ViewError> {
        match &self.attached {
            AttachedSurfaces::Rendered(renderer) => {
                Ok(renderer.send(RendererInput::CommitState { surface, built })?)
            }
            AttachedSurfaces::Unrendered(controls) => (controls.commit_state(surface, built))
                .map_err(|Closed| ViewError::Stopped(Stopped::Controls)),
        }
    }
}

pub(super) fn text_rendering_set_up_for_painter(
    setup: RendererSetup,
) -> Result<TextRendering, lavashell_renderer::Error> {
    let mut painter = Painter::without_device(&setup.fonts)?;
    if let Some(configure) = setup.painter {
        configure(&mut painter);
    }
    Ok(painter.text_rendering())
}
