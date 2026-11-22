// SPDX-License-Identifier: GPL-3.0-only
use super::links::ViewLink;
use super::requests::{Ending, Requests};
use super::{Wayland, WaylandReports};
use crate::Error;
use crate::EventLoopError;
use crate::clipboard::ClipboardState;
use crate::native::{
    Grabs, NativeSurfaces, PresentationFeedbackSource, SeatSet, SurfaceProtocols, WatchSources,
    report_user_languages,
};
use crate::sctk;
use sctk::compositor::CompositorState;
use sctk::output::OutputState;
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::registry::RegistryState;
use sctk::seat::SeatState;
use sctk::shm::Shm;

pub(crate) struct Translator {
    pub(super) focus_grabs: Option<Grabs>,
    pub(super) sources: Vec<RegistrationToken>,
    pub(super) watches: WatchSources,
    pub(super) surfaces: NativeSurfaces,
    pub(super) links: WaylandReports,
    pub(super) view: ViewLink,
    pub(super) requests: Requests,
    pub(super) ending: Ending,
    pub(super) connection: Connection,
    pub(super) qh: QueueHandle<Self>,
    pub(super) registry: RegistryState,
    pub(super) seats: SeatState,
    pub(super) outputs: OutputState,
    pub(super) shm: Shm,
    pub(super) compositor: CompositorState,
    pub(super) protocols: SurfaceProtocols,
    pub(super) devices: SeatSet,
    pub(super) clipboard: ClipboardState,
    pub(super) loop_handle: LoopHandle<'static, Self>,
    pub(super) presentation: Option<PresentationFeedbackSource>,
}

impl Drop for Translator {
    fn drop(&mut self) {
        self.surfaces.clear();
        for source in self.sources.drain(..) {
            self.loop_handle.remove(source);
        }
        self.watches.remove_all(&self.loop_handle);
    }
}

impl Translator {
    pub(super) fn new(
        wayland: Wayland,
        links: WaylandReports,
        requests: Requests,
        loop_handle: LoopHandle<'static, Self>,
    ) -> Result<Self, Error> {
        let Wayland {
            connection,
            globals,
            queue,
            protocols,
            focus_grabs,
            data_devices,
            compositor,
            shm,
            presentation,
        } = wayland;
        let qh = queue.handle();
        let seats = SeatState::new(&globals, &qh);
        let connection_source = WaylandSource::new(connection.clone(), queue)
            .insert(loop_handle.clone())
            .map_err(|failure| EventLoopError::WatchConnection(failure.into()))?;
        let view = ViewLink::new(links.view.clone());
        report_user_languages(&view);
        Ok(Self {
            focus_grabs,
            sources: vec![connection_source],
            watches: WatchSources::default(),
            surfaces: NativeSurfaces::default(),
            links,
            view,
            requests,
            ending: Ending::default(),
            connection,
            registry: RegistryState::new(&globals),
            devices: SeatSet::new(seats.seats()),
            clipboard: ClipboardState::new(data_devices, &qh, seats.seats()),
            seats,
            outputs: OutputState::new(&globals, &qh),
            shm,
            compositor,
            protocols,
            loop_handle,
            presentation,
            qh,
        })
    }
}
