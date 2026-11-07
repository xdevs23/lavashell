// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, warn_whole_scene_next};
use crate::Error;
use lavashell_actor::Closed;
use lavashell_runtime::{
    BuilderObserver, BuilderReport, BuiltChange, RendererInput, SchedulerInput,
};
use sctk::reexports::calloop::channel::{self, Event};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;
use std::mem;

/// Watches the reports of the scene builder in the event loop, and returns the observer that sends each report there.
pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<BuilderObserver, Error> {
    let (sender, channel) = channel::channel();
    let source = handle
        .insert_source(channel, |event, (), platform| match event {
            Event::Msg(report) => platform.scene_report(report),
            Event::Closed => platform.stops.scene = true,
        })
        .map_err(|failure| Error::WatchScene(failure.error))?;
    sources.push(source);
    let observer = move |report: BuilderReport| sender.send(report).map_err(|_unsent| Closed);
    Ok(Box::new(observer))
}

impl Platform {
    /// Renders a composed frame of the surface once its scene is built. A frame sent outside a compose that changed
    /// only the surface state waits for a commit without a frame, which the frame scheduler starts. After a frame
    /// that did not fit the scene, the view of the surface sends the whole scene to a restarted sink.
    fn scene_report(&mut self, report: BuilderReport) {
        match report {
            BuilderReport::Built {
                surface,
                sequence: None,
                change: BuiltChange::State,
            } => self.schedule(SchedulerInput::StateChanged(surface)),
            BuilderReport::Built {
                surface, sequence, ..
            } => {
                self.schedule(SchedulerInput::Built { surface, sequence });
            }
            BuilderReport::Restart { surface, error } => {
                warn_whole_scene_next(error);
                self.scene.restart(surface);
                if let Some(view) = self.surface_records.open_view_mut(surface) {
                    view.invalidate();
                }
            }
        }
    }

    /// Makes the renderer commit the built state in `state_commits` of each open surface without a frame, and records
    /// the panel requests that the commit applies.
    pub(super) fn commit_built_states(&mut self) -> Result<(), Error> {
        for (id, built) in mem::take(&mut self.state_commits) {
            if self.surface_records.is_open(id)
                && let Some(surface) = self.surfaces.get_mut(&id)
            {
                surface.committing = self.panels.requests(id, &built.state);
                (self.renderer).send(RendererInput::CommitState { surface: id, built })?;
            }
        }
        Ok(())
    }
}
