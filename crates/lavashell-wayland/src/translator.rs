// SPDX-License-Identifier: GPL-3.0-only

mod clipboard;
mod drag_target;
mod error;
mod focus_grab;
mod globals;
mod links;
mod outputs;
mod presentation;
pub(crate) mod requests;
mod seats;
mod session_lock;
mod state;
mod surface_events;
mod surfaces;
mod touch;

pub use error::ConnectionError;
pub use error::EventLoopError;
pub(crate) use links::ViewLink;
pub(crate) use state::Translator;

use crate::Error;
use crate::RequestsError;
use crate::bind::bind_optional;
use crate::native::{
    Control, Grabs, PresentationFeedbackSource, SurfaceProtocols, advertised_outputs,
};
use crate::sctk;
use crate::wayland_presentation::{WaylandBackend, WaylandPresentation};
use lavashell_actor::SystemClock;
use lavashell_platform::{Capability, HostFacts, Platform, PlatformLinks, Reports};
use lavashell_vulkan_renderer::VulkanBackend;
use requests::{Requests, execute_arrived};
use sctk::compositor::CompositorState;
use sctk::data_device_manager::DataDeviceManagerState;
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::ping::make_ping;
use sctk::reexports::client::globals::{GlobalList, registry_queue_init};
use sctk::reexports::client::protocol::wl_data_device_manager::WlDataDeviceManager;
use sctk::reexports::client::{Connection, EventQueue, Proxy};
use sctk::shm::Shm;
use std::collections::BTreeSet;
use std::sync::Arc;

type WaylandReports = Reports<Control, WaylandBackend>;
type WaylandLinks = PlatformLinks<Control, WaylandBackend>;

pub struct Wayland {
    connection: Connection,
    globals: GlobalList,
    queue: EventQueue<Translator>,
    protocols: SurfaceProtocols,
    focus_grabs: Option<Grabs>,
    data_devices: Option<DataDeviceManagerState>,
    compositor: CompositorState,
    shm: Shm,
    presentation: Option<PresentationFeedbackSource>,
}

impl Platform for Wayland {
    type Setup = ();
    type Error = Error;
    type Backend = WaylandBackend;
    type Control = Control;

    fn connect((): ()) -> Result<lavashell_platform::Connection<Self>, Error> {
        let connection = Connection::connect_to_env().map_err(ConnectionError::Connect)?;
        let (globals, queue) =
            registry_queue_init::<Translator>(&connection).map_err(ConnectionError::ReadGlobals)?;
        let qh = queue.handle();
        let presentation = PresentationFeedbackSource::bind(&globals, &qh);
        let host = HostFacts {
            outputs: advertised_outputs(&globals),
            reports_presentation: presentation.is_some(),
        };
        let backend =
            VulkanBackend::without_streams(WaylandPresentation::new(connection.backend()));
        let wayland = Self {
            protocols: SurfaceProtocols::bind(&globals, &qh),
            focus_grabs: Grabs::bind(&globals, &qh),
            data_devices: bind_optional(
                WlDataDeviceManager::interface().name,
                DataDeviceManagerState::bind(&globals, &qh),
            ),
            compositor: CompositorState::bind(&globals, &qh)
                .map_err(ConnectionError::BindCompositor)?,
            shm: Shm::bind(&globals, &qh).map_err(ConnectionError::BindShm)?,
            presentation,
            globals,
            queue,
            connection,
        };
        Ok(lavashell_platform::Connection {
            platform: wayland,
            backend: Some(backend),
            host,
            clock: Arc::new(SystemClock::start()),
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        let transfers = self.data_devices.is_some();
        let optional = [
            self.focus_grabs.is_some().then_some(Capability::FocusGrab),
            transfers.then_some(Capability::Clipboard),
            transfers.then_some(Capability::DragAndDrop),
        ];
        (self.protocols.capabilities())
            .chain(optional.into_iter().flatten())
            .chain([Capability::Accessibility])
            .collect()
    }

    fn serve(self, links: WaylandLinks) -> Result<(), Error> {
        let PlatformLinks { reports, requests } = links;
        let mut event_loop: EventLoop<'static, Translator> =
            EventLoop::try_new().map_err(|error| EventLoopError::CreateEventLoop(error.into()))?;
        let handle = event_loop.handle();
        let (wake, arrivals) = make_ping().map_err(RequestsError::CreateRequestWake)?;
        let arrival = wake.clone();
        requests.on_arrival(move || arrival.ping());
        let intake = requests.intake();
        let source = handle
            .insert_source(arrivals, move |(), (), translator: &mut Translator| {
                let execute = |request| translator.execute(request);
                if let Some(end) = execute_arrived(&requests, &wake, execute) {
                    translator.ending.record(end);
                }
            })
            .map_err(|failure| RequestsError::WatchRequests(failure.into()))?;
        let requests = Requests::new(intake, source);
        let mut translator = Translator::new(self, reports, requests, handle)?;
        loop {
            let served = (event_loop.dispatch(None, &mut translator))
                .map_err(|error| EventLoopError::DispatchEvents(error.into()))
                .map_err(Error::from)
                .and_then(|()| translator.flush());
            if let Err(failure) = served {
                translator.ending.record(Err(failure));
            }
            if let Some(end) = translator.ending.take() {
                return end;
            }
        }
    }
}
