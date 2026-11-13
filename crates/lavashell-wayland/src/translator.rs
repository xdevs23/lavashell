// SPDX-License-Identifier: GPL-3.0-only
//! The platform of a Wayland compositor. It connects on the thread that starts the run and serves the connection on
//! a thread of its own, where it translates the events of the compositor and executes the requests of the runtime.

mod links;
mod requests;
mod state;
mod surfaces;

pub(crate) use links::ViewLink;
pub(crate) use state::Translator;

use crate::Error;
use crate::app::{Control, Presentation, SurfaceProtocols, advertised_outputs};
use crate::wayland_presentation::WaylandPresentation;
use lavashell_platform::{HostFacts, Platform, PlatformLinks};
use links::Links;
use requests::{Requests, execute_arrived};
use sctk::compositor::CompositorState;
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::ping::make_ping;
use sctk::reexports::client::globals::{GlobalList, registry_queue_init};
use sctk::reexports::client::{Connection, EventQueue};
use sctk::shm::Shm;
use smithay_client_toolkit as sctk;

/// The connection to a Wayland compositor and the protocols bound on it, until the platform serves it.
pub struct Wayland {
    connection: Connection,
    globals: GlobalList,
    queue: EventQueue<Translator>,
    protocols: SurfaceProtocols,
    compositor: CompositorState,
    shm: Shm,
    presentation: Option<Presentation>,
}

impl Platform for Wayland {
    type Error = Error;
    type Presentation = WaylandPresentation;
    type Control = Control;

    fn connect() -> Result<lavashell_platform::Connection<Self>, Error> {
        let connection = Connection::connect_to_env().map_err(Error::Connect)?;
        let (globals, queue) =
            registry_queue_init::<Translator>(&connection).map_err(Error::ReadGlobals)?;
        let qh = queue.handle();
        let presentation = Presentation::bind(&globals, &qh);
        let host = HostFacts {
            outputs: advertised_outputs(&globals),
            reports_presentation: presentation.is_some(),
        };
        let backend = WaylandPresentation::new(connection.backend());
        let wayland = Self {
            protocols: SurfaceProtocols::bind(&globals, &qh),
            compositor: CompositorState::bind(&globals, &qh).map_err(Error::BindCompositor)?,
            shm: Shm::bind(&globals, &qh).map_err(Error::BindShm)?,
            presentation,
            globals,
            queue,
            connection,
        };
        Ok(lavashell_platform::Connection {
            platform: wayland,
            presentation: backend,
            host,
        })
    }

    fn serve(self, links: PlatformLinks<Control>) -> Result<(), Error> {
        let PlatformLinks { reports, requests } = links;
        let mut event_loop: EventLoop<'static, Translator> =
            EventLoop::try_new().map_err(Error::CreateEventLoop)?;
        let handle = event_loop.handle();
        let (wake, arrivals) = make_ping().map_err(Error::CreateRequestWake)?;
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
            .map_err(|failure| Error::WatchRequests(failure.error))?;
        let requests = Requests::new(intake, source);
        let mut translator = Translator::new(self, Links::new(reports), requests, handle)?;
        loop {
            let served = (event_loop.dispatch(None, &mut translator))
                .map_err(Error::DispatchEvents)
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
