// SPDX-License-Identifier: GPL-3.0-only
use super::{StartError, Stopped, ViewError, ViewInput, observer};
use crate::renderer_actor::{PlatformRendererError, Renderer, RendererStopped};
use crate::scene_pipeline::{self, ScenePipeline};
use crate::scheduler::Redraw;
use crate::scheduler_actor::{self, SchedulerThread};
use lavashell_actor::{Clock, JoinError, Outlet, Sender};
use lavashell_platform::{Platform, PlatformEvent, Reports, StreamSetup};
use lavashell_renderer::RendererSetup;
use std::sync::Arc;

/// The actors between the views and the host, which report to the inbox of the views.
pub(super) struct Actors<P: Platform> {
    pub(super) scene: ScenePipeline,
    pub(super) renderer: Renderer<P::Control, P::Streams>,
    pub(super) scheduler: SchedulerThread,
}

/// What each actor returned when it stopped.
pub(crate) struct ActorStops<P: Platform> {
    pub(crate) renderer: Result<(), JoinError<PlatformRendererError<P>>>,
    pub(crate) scheduler: Result<(), scheduler_actor::Error>,
    pub(crate) scene: Result<(), Arc<scene_pipeline::Error>>,
}

impl<P: Platform> Actors<P> {
    /// Starts the scene pipeline, the frame scheduler for `redraw` and the renderer, which measure time on `clock`
    /// and send their reports to `inbox`. The renderer uses `presentation`, `setup` and `streams`, and measures the CPU
    /// time of each draw when `cpu_timing` is set.
    ///
    /// # Errors
    ///
    /// Fails when an actor cannot start.
    pub(super) fn start(
        presentation: P::Presentation,
        setup: RendererSetup,
        streams: StreamSetup<P::Streams>,
        redraw: Redraw,
        clock: &Arc<dyn Clock>,
        cpu_timing: bool,
        inbox: &Sender<ViewInput>,
    ) -> Result<Self, StartError<P>> {
        let builder = observer(inbox, ViewInput::Builder, Stopped::Scene);
        let scene = ScenePipeline::start(builder, clock.as_ref()).map_err(ViewError::from)?;
        let scheduler_outputs = observer(inbox, ViewInput::Scheduler, Stopped::Scheduler);
        let scheduler = SchedulerThread::start(redraw, clock.clone(), scheduler_outputs)
            .map_err(ViewError::from)?;
        let outputs = observer(inbox, ViewInput::Renderer, Stopped::Renderer);
        let renderer = Renderer::start(
            presentation,
            setup,
            streams,
            clock.clone(),
            cpu_timing,
            outputs,
        )
        .map_err(StartError::StartRenderer)?;
        Ok(Self {
            scene,
            renderer,
            scheduler,
        })
    }

    /// The links through which a platform reports to the views through `view`, to the frame scheduler and to the
    /// renderer, with the times measured on `clock`.
    ///
    /// # Errors
    ///
    /// Fails when the renderer stopped.
    pub(super) fn reports(
        &self,
        view: Outlet<PlatformEvent>,
        clock: Arc<dyn Clock>,
    ) -> Result<Reports<P::Control, P::Streams>, RendererStopped> {
        Ok(Reports {
            view,
            scheduler: self.scheduler.timing_outlet(),
            renderer: self.renderer.attachments()?,
            streams: self.renderer.stream_tasks()?,
            clock,
        })
    }
}
