// SPDX-License-Identifier: GPL-3.0-only
use super::{Platform, actor_link, warn_whole_scene_next};
use crate::Error;
use lavashell_runtime::{BuilderObserver, BuilderReport, SchedulerInput};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;

/// Watches the reports of the scene builder in the event loop, and returns the observer that sends each report there.
pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<BuilderObserver, Error> {
    let observer = actor_link::watch(
        handle,
        sources,
        Platform::scene_report,
        |platform| platform.scene_stopped = true,
        Error::WatchScene,
    )?;
    Ok(Box::new(observer))
}

impl Platform {
    /// Renders a composed frame of the surface once its scene is built, and commits a surface state built without a
    /// frame with the next render. After a frame that did not fit the scene, the view of the surface sends the whole
    /// scene to a restarted sink.
    fn scene_report(&mut self, report: BuilderReport) {
        match report {
            BuilderReport::Built { surface, sequence } => {
                self.schedule(SchedulerInput::Built { surface, sequence });
            }
            BuilderReport::StateChanged { surface } => {
                if let Some(surface) = self.surfaces.get_mut(&surface) {
                    surface.uncommitted_state = true;
                }
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
}
