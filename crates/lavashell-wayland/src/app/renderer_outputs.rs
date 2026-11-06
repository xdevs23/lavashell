// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use lavashell_actor::Closed;
use lavashell_core::surface::SurfaceId;
use lavashell_runtime::{RendererObserver, RendererOutput, SchedulerInput};
use sctk::reexports::calloop::channel::{self, Event};
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay_client_toolkit as sctk;

/// Watches the outputs of the renderer in the event loop, and returns the observer that sends each output there.
pub(super) fn install(
    handle: &LoopHandle<'static, Platform>,
    sources: &mut Vec<RegistrationToken>,
) -> Result<RendererObserver, Error> {
    let (sender, channel) = channel::channel();
    let source = handle
        .insert_source(channel, |event, (), platform| match event {
            Event::Msg(output) => platform.renderer_output(output),
            Event::Closed => platform.stops.renderer = true,
        })
        .map_err(|failure| Error::WatchRenderer(failure.error))?;
    sources.push(source);
    let observer = move |output: RendererOutput| sender.send(output).map_err(|_unsent| Closed);
    Ok(Box::new(observer))
}

impl Platform {
    fn renderer_output(&mut self, output: RendererOutput) {
        match output {
            RendererOutput::Rendered(rendered) => self.rendered(&rendered),
            RendererOutput::Committed(id) => self.committed(id),
            RendererOutput::Retired(id) => self.retired(id),
            RendererOutput::SceneWake(deadline) => self.scene_deadline = deadline,
        }
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

    /// Destroys the protocol objects of surface `id` once the renderer reports that nothing presents to the surface
    /// anymore. Then drops the scene and the view of the surface.
    fn retired(&mut self, id: SurfaceId) {
        drop(self.retiring.remove(&id));
        if self.scene.retire(id).is_err() {
            self.stops.scene = true;
        }
        self.surface_records.retire(id);
    }
}
