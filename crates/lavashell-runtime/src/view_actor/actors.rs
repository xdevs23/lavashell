// SPDX-License-Identifier: GPL-3.0-only
use super::{Stopped, ViewError, ViewInput, observer};
use crate::cpu_time::CpuTiming;
use crate::rendered_platform::{PlatformStreams, RenderedPlatform};
use crate::renderer_actor::{PlatformRendererError, Renderer, RendererStopped};
use crate::run::RunError;
use crate::scene_pipeline::{self, ScenePipeline};
use crate::scheduler::Redraw;
use crate::scheduler_actor::{self, SchedulerThread};
use lavashell_actor::{Clock, JoinError, Outlet, Sender};
use lavashell_platform::{PlatformEvent, Reports};
use lavashell_vulkan_renderer::RendererSetup;
use std::sync::Arc;

pub(super) struct Actors<P: RenderedPlatform> {
    pub(super) scene: ScenePipeline,
    pub(super) renderer: Renderer<P::Control, PlatformStreams<P>>,
    pub(super) scheduler: SchedulerThread,
}

pub(crate) struct ActorStops<P: RenderedPlatform> {
    pub(crate) renderer: Result<(), JoinError<PlatformRendererError<P>>>,
    pub(crate) scheduler: Result<(), scheduler_actor::Error>,
    pub(crate) scene: Result<(), Arc<scene_pipeline::Error>>,
}

pub(super) struct ActorSetup<P: RenderedPlatform> {
    pub(super) backend: P::Backend,
    pub(super) renderer: RendererSetup,
    pub(super) redraw: Redraw,
    pub(super) cpu_timing: CpuTiming,
}

impl<P: RenderedPlatform> Actors<P> {
    pub(super) fn start(
        setup: ActorSetup<P>,
        clock: &Arc<dyn Clock>,
        inbox: &Sender<ViewInput>,
    ) -> Result<Self, RunError<P>> {
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
        let outputs = observer(inbox, ViewInput::Renderer, Stopped::Renderer);
        let renderer = Renderer::start(backend, renderer_setup, clock.clone(), cpu_timing, outputs)
            .map_err(RunError::StartRenderer)?;
        Ok(Self {
            scene,
            renderer,
            scheduler,
        })
    }

    pub(super) fn reports(
        &self,
        view: Outlet<PlatformEvent>,
        clock: Arc<dyn Clock>,
    ) -> Result<Reports<P::Control, P::Backend>, RendererStopped> {
        Ok(Reports {
            view,
            scheduler: self.scheduler.timing_outlet(),
            renderer: self.renderer.attachments()?,
            streams: self.renderer.stream_tasks()?,
            clock,
        })
    }
}
