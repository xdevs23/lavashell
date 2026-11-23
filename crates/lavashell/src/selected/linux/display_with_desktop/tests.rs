// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Outlet, SystemClock, mailbox};
use lavashell_desktop::{DesktopService, ServiceWatch, plug_in};
use lavashell_platform::{HostFacts, PlatformEvent, Reports, SurfaceControl};
use lavashell_vulkan_renderer::HeadlessBackend;
use std::convert::Infallible;
use std::sync::Arc;
use std::{future, panic};

#[derive(Debug, thiserror::Error)]
#[error("the stand-in display fails")]
struct DisplayFailed;

impl NamesNextStep for DisplayFailed {
    fn next_step(&self) -> NextStep {
        NextStep::UseSupportingPlatform
    }
}

struct NoControl;

impl SurfaceControl for NoControl {
    type Error = Infallible;
    type Message = ();

    fn receive(&mut self, (): ()) {}
}

struct StandInDisplay {
    served: Result<(), DisplayFailed>,
}

impl Platform for StandInDisplay {
    type Setup = ();
    type Error = DisplayFailed;
    type Backend = HeadlessBackend;
    type Control = NoControl;

    fn connect((): ()) -> Result<Connection<Self>, DisplayFailed> {
        Ok(Connection {
            platform: Self { served: Ok(()) },
            backend: None,
            host: HostFacts {
                outputs: Vec::new(),
                reports_presentation: false,
            },
            clock: Arc::new(SystemClock::start()),
        })
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        BTreeSet::new()
    }

    fn serve(self, _links: PlatformLinks<NoControl, HeadlessBackend>) -> Result<(), DisplayFailed> {
        self.served
    }
}

fn links() -> PlatformLinks<NoControl, HeadlessBackend> {
    let (_requests, requests) = mailbox();
    PlatformLinks {
        reports: Reports {
            view: Outlet::new(|_| Ok(())),
            scheduler: Outlet::new(|_| Ok(())),
            renderer: Outlet::new(|_| Ok(())),
            streams: Outlet::new(|_| Ok(())),
            clock: Arc::new(SystemClock::start()),
        },
        requests,
    }
}

#[test]
fn serve_returns_the_error_that_the_display_returns() {
    let failing = DisplayWithDesktop {
        display: StandInDisplay {
            served: Err(DisplayFailed),
        },
        services: DesktopServices::set_up(&Store::default()),
    };
    assert!(matches!(
        failing.serve(links()),
        Err(DisplayWithDesktopError::Display(DisplayFailed))
    ));
}

#[test]
fn the_error_names_the_same_next_step_as_the_error_it_wraps() {
    assert_eq!(
        DisplayWithDesktopError::Display(DisplayFailed).next_step(),
        DisplayFailed.next_step()
    );
    let failure = ServicesStartError::CreateStop(std::io::Error::from_raw_os_error(24));
    let step = failure.next_step();
    let services: DisplayWithDesktopError<DisplayFailed> =
        DisplayWithDesktopError::StartServices(failure);
    assert_eq!(services.next_step(), step);
}

#[test]
fn a_store_without_the_desktop_layer_connects_and_serves_the_display()
-> Result<(), DisplayWithDesktopError<DisplayFailed>> {
    let Connection { platform, .. } =
        DisplayWithDesktop::<StandInDisplay>::connect(Store::default())?;
    platform.serve(links())
}

struct WatchesForever;

impl DesktopService for WatchesForever {
    fn into_watches(self: Box<Self>, _view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        vec![Box::pin(future::pending())]
    }
}

struct PanicsWhileStarting;

impl DesktopService for PanicsWhileStarting {
    fn into_watches(self: Box<Self>, _view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        panic::resume_unwind(Box::new("the service panics while its watches start"))
    }
}

fn connect_and_serve_with(
    construct: fn(&Store) -> Box<dyn DesktopService>,
) -> Result<(), DisplayWithDesktopError<DisplayFailed>> {
    let mut registered = Store::default();
    plug_in(&mut registered, construct);
    let Connection { platform, .. } = DisplayWithDesktop::<StandInDisplay>::connect(registered)?;
    platform.serve(links())
}

#[test]
fn serve_returns_once_the_display_returns_while_a_registered_service_still_watches() {
    assert!(matches!(
        connect_and_serve_with(|_| Box::new(WatchesForever)),
        Ok(())
    ));
}

#[test]
fn serve_fails_when_a_registered_service_panics_while_its_watches_start() {
    assert!(matches!(
        connect_and_serve_with(|_| Box::new(PanicsWhileStarting)),
        Err(DisplayWithDesktopError::StartServices(
            ServicesStartError::Panicked { .. }
        ))
    ));
}
