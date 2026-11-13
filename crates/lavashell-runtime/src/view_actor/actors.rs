// SPDX-License-Identifier: GPL-3.0-only
use super::{StartError, Stopped, ViewError, ViewInput, observer};
use crate::{
    Error, Redraw, Renderer, RendererError, RendererStopped, ScenePipeline, SchedulerThread,
};
use lavashell_actor::{JoinError, Outlet, Sender};
use lavashell_platform::{SurfaceAttachment, TimingEvent};
use lavashell_renderer::{PresentationBackend, RendererSetup, SurfaceControl};
use std::sync::Arc;
use std::time::Instant;

/// The actors between the views and the host, which report to the inbox of the views.
pub(super) struct Actors<C: SurfaceControl + Send + 'static> {
    pub(super) scene: ScenePipeline,
    pub(super) renderer: Renderer<C>,
    pub(super) scheduler: SchedulerThread,
}

/// What each actor returned when it stopped. `E` is the error of the controls of the native surfaces.
pub(crate) struct ActorStops<E> {
    pub(crate) renderer: Result<(), JoinError<RendererError<E>>>,
    pub(crate) scheduler: Result<(), Error>,
    pub(crate) scene: Result<(), Arc<Error>>,
}

impl<C: SurfaceControl + Send + 'static> Actors<C> {
    /// Starts the scene pipeline, the frame scheduler for `redraw` and the renderer for the surfaces of `presentation`
    /// from `setup`, which measure time from `start` and report to `inbox`. The renderer measures the CPU time of each
    /// draw when `cpu_timing` is set.
    ///
    /// # Errors
    ///
    /// Fails when an actor cannot start.
    pub(super) fn start(
        presentation: impl PresentationBackend + Send + 'static,
        setup: RendererSetup,
        redraw: Redraw,
        start: Instant,
        cpu_timing: bool,
        inbox: &Sender<ViewInput>,
    ) -> Result<Self, StartError<C::Error>> {
        let builder = observer(inbox, ViewInput::Builder, Stopped::Scene);
        let scene = ScenePipeline::start(builder).map_err(ViewError::Runtime)?;
        let scheduler_outputs = observer(inbox, ViewInput::Scheduler, Stopped::Scheduler);
        let scheduler =
            SchedulerThread::start(redraw, start, scheduler_outputs).map_err(ViewError::Runtime)?;
        let outputs = observer(inbox, ViewInput::Renderer, Stopped::Renderer);
        let renderer = Renderer::start(presentation, setup, start, cpu_timing, outputs)
            .map_err(StartError::StartRenderer)?;
        Ok(Self {
            scene,
            renderer,
            scheduler,
        })
    }

    /// The outlets through which a platform reports the timing of the frames to the frame scheduler and attaches its
    /// native surfaces to the renderer.
    ///
    /// # Errors
    ///
    /// Fails when the renderer stopped.
    pub(super) fn links(
        &self,
    ) -> Result<(Outlet<TimingEvent>, Outlet<SurfaceAttachment<C>>), RendererStopped> {
        Ok((self.scheduler.timing_outlet(), self.renderer.attachments()?))
    }
}
