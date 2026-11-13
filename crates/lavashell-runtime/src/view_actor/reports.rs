// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use crate::renderer_actor::{RendererInput, RendererOutput};
use crate::scene_pipeline::BuilderReport;
use crate::scheduler::{SchedulerInput, SchedulerOutput};
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{PlatformRequest, SurfaceRequest};
use lavashell_renderer::SurfaceControl;

impl<C: SurfaceControl + Send + 'static> ViewActor<C> {
    /// Renders a composed frame of the surface once its scene is built. A surface state built without a frame waits
    /// for a commit without a frame, which the frame scheduler starts. After a frame that did not fit the scene, the
    /// view of the surface sends the whole scene to a restarted sink.
    pub(super) fn scene_report(&mut self, report: BuilderReport) {
        match report {
            BuilderReport::Built { surface, sequence } => {
                self.schedule(SchedulerInput::Built { surface, sequence });
            }
            BuilderReport::StateChanged { surface } => {
                self.schedule(SchedulerInput::StateChanged(surface));
            }
            BuilderReport::Restart { surface, error } => {
                if let Some(view) = self.records.open_view_mut(surface) {
                    self.actors.scene.restart_view(surface, view, &error);
                }
            }
        }
    }

    pub(super) fn renderer_output(&mut self, output: RendererOutput) -> Result<(), ViewError> {
        match output {
            RendererOutput::Rendered(rendered) => self.rendered(&rendered),
            RendererOutput::Committed(id) => self.committed(id),
            RendererOutput::Retired(id) => self.retired(id)?,
            RendererOutput::SceneWake(deadline) => self.pacing.set_scene_deadline(deadline),
        }
        Ok(())
    }

    /// Stores each started frame and each commit without a frame for the next render, and the time at which the views
    /// report again. Passes each report to the frame observer of the app. A commit reads the newest built state of
    /// the surface when the scheduler starts it, while no frame of the surface is composed or drawn.
    pub(super) fn scheduler_output(&mut self, output: SchedulerOutput) {
        match output {
            SchedulerOutput::Frame {
                surface,
                kind,
                time,
            } => self.pacing.start_frame(surface, kind, time),
            SchedulerOutput::CommitState(surface) => {
                let built = self.actors.scene.built(surface);
                self.pacing.commit_state(surface, built);
            }
            SchedulerOutput::Report(report) => {
                if let Some(observer) = &mut self.observers.frame_report {
                    observer(*report);
                }
            }
            SchedulerOutput::WakeAt(wake) => self.pacing.set_scheduler_wake(wake),
        }
    }

    /// Makes the renderer commit the built state that waits for a commit without a frame of each open surface, and
    /// records the panel requests that the commit applies.
    pub(super) fn commit_built_states(&mut self) -> Result<(), ViewError> {
        for (id, built) in self.pacing.take_state_commits() {
            if self.records.is_open(id)
                && let Some(surface) = self.surfaces.get_mut(&id)
            {
                surface.committing = self.panels.requests(id, &built.state);
                (self.actors.renderer).send(RendererInput::CommitState { surface: id, built })?;
            }
        }
        Ok(())
    }

    /// Records the panel requests applied by the commit without a frame of surface `id`, and reports the commit to the
    /// frame scheduler.
    fn committed(&mut self, id: SurfaceId) {
        let committing = (self.surfaces.get_mut(&id)).and_then(|surface| surface.committing.take());
        if let Some(requests) = committing {
            self.panels.committed(id, requests);
        }
        self.schedule(SchedulerInput::Committed(id));
    }

    /// Asks the platform to release the native surface `id` once the renderer reports that nothing presents to the
    /// surface anymore. Then drops the scene and the view of the surface.
    fn retired(&mut self, id: SurfaceId) -> Result<(), ViewError> {
        self.request(PlatformRequest::Surface(SurfaceRequest::Release(id)))?;
        self.actors
            .scene
            .retire(id)
            .map_err(|Closed| ViewError::Stopped(Stopped::Scene))?;
        self.records.retire(id);
        Ok(())
    }
}
