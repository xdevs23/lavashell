// SPDX-License-Identifier: GPL-3.0-only
use super::Wayland;
use super::links::Links;
use super::requests::{Ending, Requests};
use super::surfaces::NativeSurfaces;
use crate::Error;
use crate::EventLoopError;
use crate::app::{
    Grabs, Presentation, SeatDevices, Surface, SurfaceProtocols, WatchSources, install_preferences,
};
use crate::clipboard::ClipboardState;
use lavashell_core::surface::SurfaceId;
use sctk::compositor::CompositorState;
use sctk::output::OutputState;
use sctk::reexports::calloop::{LoopHandle, RegistrationToken};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::ext::session_lock::v1::client::ext_session_lock_v1::ExtSessionLockV1;
use sctk::registry::RegistryState;
use sctk::seat::SeatState;
use sctk::shm::Shm;
use smithay_client_toolkit as sctk;

/// The state that translates the events of the compositor into reports to the runtime and executes the requests of
/// the runtime.
pub(crate) struct Translator {
    pub(crate) focus_grabs: Option<Grabs>,
    pub(crate) session_lock: Option<ExtSessionLockV1>,
    sources: Vec<RegistrationToken>,
    pub(crate) watches: WatchSources,
    pub(crate) surfaces: NativeSurfaces<Surface>,
    pub(crate) links: Links,
    pub(super) requests: Requests,
    pub(super) ending: Ending,
    pub(crate) connection: Connection,
    pub(crate) qh: QueueHandle<Self>,
    pub(crate) registry: RegistryState,
    pub(crate) seats: SeatState,
    pub(crate) outputs: OutputState,
    pub(crate) shm: Shm,
    pub(crate) compositor: CompositorState,
    pub(crate) protocols: SurfaceProtocols,
    pub(crate) devices: Vec<SeatDevices>,
    pub(crate) clipboard: ClipboardState,
    pub(crate) loop_handle: LoopHandle<'static, Self>,
    pub(crate) presentation: Option<Presentation>,
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
    /// The state that translates the events of the connection of `wayland` on the event loop of `loop_handle`,
    /// reports them through `links` and executes the requests of `requests`.
    pub(super) fn new(
        wayland: Wayland,
        links: Links,
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
            .map_err(|failure| EventLoopError::WatchConnection(failure.error))?;
        let mut sources = vec![connection_source];
        install_preferences(&loop_handle, &mut sources, &links.view())?;
        Ok(Self {
            focus_grabs,
            session_lock: None,
            sources,
            watches: WatchSources::default(),
            surfaces: NativeSurfaces::default(),
            links,
            requests,
            ending: Ending::default(),
            connection,
            registry: RegistryState::new(&globals),
            devices: seats.seats().map(SeatDevices::new).collect(),
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

    /// The id of the live surface whose native surface is `wl`.
    pub(crate) fn find_surface(&self, wl: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surfaces.find_live(|known| &known.wl == wl)
    }
}
