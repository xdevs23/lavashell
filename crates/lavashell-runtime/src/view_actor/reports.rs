// SPDX-License-Identifier: GPL-3.0-only
use super::ViewActor;
use crate::controls_actor::ControlsOutput;
use crate::renderer_actor::RendererOutput;
use crate::scene_pipeline::BuilderReport;
use crate::scheduler::{SchedulerInput, SchedulerOutput};
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Platform, PlatformRequest, SurfaceRequest};

impl<P: Platform> ViewActor<P> {
    pub(super) fn scene_report(&mut self, report: BuilderReport) {
        match report {
            BuilderReport::Built { surface, sequence } => {
                self.schedule(SchedulerInput::Built { surface, sequence });
            }
            BuilderReport::StateChanged { surface } => {
                self.schedule(SchedulerInput::StateChanged(surface));
            }
            BuilderReport::Restart { surface, error } => {
                if let Some(view) = self.surfaces.records_mut().open_view_mut(surface) {
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

    pub(super) fn controls_output(&mut self, output: ControlsOutput) -> Result<(), ViewError> {
        match output {
            ControlsOutput::Committed(id) => self.committed(id),
            ControlsOutput::Retired(id) => self.retired(id)?,
        }
        Ok(())
    }

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

    pub(super) fn commit_built_states(&mut self) -> Result<(), ViewError> {
        for (id, built) in self.pacing.take_state_commits() {
            let requests = self.surfaces.panel_requests(id, &built.state);
            if let Some(surface) = self.surfaces.open_view_surface(id) {
                surface.start_commit(requests);
                self.actors.commit_state(id, built)?;
            }
        }
        Ok(())
    }

    fn committed(&mut self, id: SurfaceId) {
        if let Some(frame) = self.surfaces.committed(id)
            && let Some(view) = self.surfaces.records().open_view(id)
        {
            self.observers.observe_view(id, frame, view);
        }
        self.schedule(SchedulerInput::Committed(id));
    }

    fn retired(&mut self, id: SurfaceId) -> Result<(), ViewError> {
        self.request(PlatformRequest::Surface(SurfaceRequest::Release(id)))?;
        self.actors
            .scene
            .retire(id)
            .map_err(|Closed| ViewError::Stopped(Stopped::Scene))?;
        self.surfaces.records_mut().retire(id);
        Ok(())
    }
}
