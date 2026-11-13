// SPDX-License-Identifier: GPL-3.0-only
use crate::panels::PanelRecords;
use crate::scene_pipeline::{FrameStatus, SceneSink};
use crate::view_actor::{Stopped, ViewError};
use lavashell_actor::Closed;
use lavashell_core::surface::{SurfaceChange, SurfaceId};
use lavashell_core::view::{Failures, FrameError, View};

/// Runs `frame` to send the next frame of `view` into `sink`, and passes the components that failed in the frame to
/// `failed`. A failed frame ends the run when no earlier frame ended into the sink, because the surface then has no
/// scene to keep showing. Otherwise the failure is logged, and the view sends the whole scene with the next frame.
pub(super) fn send_frame(
    view: &mut View,
    mut sink: SceneSink<'_>,
    failed: impl FnOnce(Failures),
    frame: impl FnOnce(&mut View, &mut SceneSink<'_>) -> Result<Failures, FrameError<Closed>>,
) -> Result<(), ViewError> {
    let shows_scene = sink.any_frame_ended();
    match frame(view, &mut sink) {
        Ok(failures) => {
            failed(failures);
            Ok(())
        }
        Err(FrameError::Sink(Closed)) => Err(ViewError::Stopped(Stopped::Scene)),
        Err(error) if shows_scene => {
            warn_whole_scene_next(&error);
            Ok(())
        }
        Err(error) => Err(ViewError::ShowScene(error)),
    }
}

/// Logs `error`, after which the view sends the whole scene with the next frame.
fn warn_whole_scene_next(error: impl std::error::Error) {
    log::warn!(
        "a frame failed, and the whole scene is sent with the next frame: {}",
        lavashell_core::error::message_chain(error)
    );
}

/// Sends `changes` and the geometry of the panel of surface `id` to `sink`, and ends the frame as
/// [`SceneSink::end_state_frame`] does.
pub(super) fn send_surface_state(
    mut sink: SceneSink<'_>,
    panels: &PanelRecords,
    id: SurfaceId,
    changes: impl IntoIterator<Item = SurfaceChange>,
) -> Result<FrameStatus, ViewError> {
    let stopped = |Closed| ViewError::Stopped(Stopped::Scene);
    for change in changes.into_iter().chain(panels.geometry_change(id)) {
        sink.set_state(change).map_err(stopped)?;
    }
    sink.end_state_frame().map_err(stopped)
}
