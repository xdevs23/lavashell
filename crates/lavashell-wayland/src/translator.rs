// SPDX-License-Identifier: GPL-3.0-only
//! The platform of a Wayland compositor. It connects on the thread that starts the run and serves the connection on
//! a thread of its own, where it translates the events of the compositor and executes the requests of the runtime.

mod error;
mod links;
pub(crate) mod requests;
mod state;
mod surfaces;

pub use error::ConnectionError;
pub use error::EventLoopError;
pub(crate) use links::ViewLink;
pub(crate) use state::Translator;

use crate::Error;
use crate::RequestsError;
use crate::app::{Control, Grabs, Presentation, SurfaceProtocols, advertised_outputs};
use crate::wayland_presentation::WaylandPresentation;
use lavashell_actor::SystemClock;
use lavashell_platform::{Capability, HostFacts, Platform, PlatformLinks};
use links::Links;
use requests::{Requests, execute_arrived};
use sctk::compositor::CompositorState;
use sctk::data_device_manager::DataDeviceManagerState;
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::ping::make_ping;
use sctk::reexports::client::globals::{GlobalList, registry_queue_init};
use sctk::reexports::client::{Connection, EventQueue};
use sctk::shm::Shm;
use smithay_client_toolkit as sctk;
use std::collections::BTreeSet;
use std::sync::Arc;

/// The connection to a Wayland compositor and the protocols bound on it, until the platform serves it.
pub struct Wayland {
    connection: Connection,
    globals: GlobalList,
    queue: EventQueue<Translator>,
    protocols: SurfaceProtocols,
    focus_grabs: Option<Grabs>,
    data_devices: Option<DataDeviceManagerState>,
    compositor: CompositorState,
    shm: Shm,
    presentation: Option<Presentation>,
}

impl Platform for Wayland {
    type Error = Error;
    type Presentation = WaylandPresentation;
    type Control = Control;

    fn connect() -> Result<lavashell_platform::Connection<Self>, Error> {
        let connection = Connection::connect_to_env().map_err(ConnectionError::Connect)?;
        let (globals, queue) =
            registry_queue_init::<Translator>(&connection).map_err(ConnectionError::ReadGlobals)?;
        let qh = queue.handle();
        let presentation = Presentation::bind(&globals, &qh);
        let host = HostFacts {
            outputs: advertised_outputs(&globals),
            reports_presentation: presentation.is_some(),
        };
        let backend = WaylandPresentation::new(connection.backend());
        let wayland = Self {
            protocols: SurfaceProtocols::bind(&globals, &qh),
            focus_grabs: Grabs::bind(&globals, &qh),
            data_devices: DataDeviceManagerState::bind(&globals, &qh).ok(),
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
            presentation: backend,
            host,
            clock: Arc::new(SystemClock::start()),
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        let transfers = self.data_devices.is_some();
        let bound = [
            (self.focus_grabs.is_some(), Capability::FocusGrab),
            (transfers, Capability::Clipboard),
            (transfers, Capability::DragAndDrop),
            (true, Capability::Accessibility),
        ];
        (self.protocols.capabilities().into_iter())
            .chain(bound)
            .filter_map(|(bound, capability)| bound.then_some(capability))
            .collect()
    }

    fn serve(self, links: PlatformLinks<Control>) -> Result<(), Error> {
        let PlatformLinks { reports, requests } = links;
        let mut event_loop: EventLoop<'static, Translator> =
            EventLoop::try_new().map_err(EventLoopError::CreateEventLoop)?;
        let handle = event_loop.handle();
        let (wake, arrivals) = make_ping().map_err(RequestsError::CreateRequestWake)?;
        let arrival = wake.clone();
        requests.on_arrival(move || arrival.ping());
        // The requests that arrived before the wake was set woke nothing.
        wake.ping();
        let intake = requests.intake();
        let source = handle
            .insert_source(arrivals, move |(), (), translator: &mut Translator| {
                let execute = |request| translator.execute(request);
                if let Some(end) = execute_arrived(&requests, &wake, execute) {
                    translator.ending.record(end);
                }
            })
            .map_err(|failure| RequestsError::WatchRequests(failure.error))?;
        let requests = Requests::new(intake, source);
        let mut translator = Translator::new(self, Links::new(reports), requests, handle)?;
        loop {
            let served = (event_loop.dispatch(None, &mut translator))
                .map_err(EventLoopError::DispatchEvents)
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
