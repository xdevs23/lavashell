// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::services::{PanicValue, ServiceWatch};
use lavashell_actor::{Outlet, SystemClock, mailbox};
use lavashell_platform::{HostFacts, PlatformEvent, Reports, SurfaceControl};
use lavashell_vulkan_renderer::HeadlessBackend;
use std::convert::Infallible;
use std::future;
use std::panic;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
#[error("the stand-in display fails")]
struct DisplayFailed;

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

fn serve(
    served: Result<(), DisplayFailed>,
    service: impl DesktopService + 'static,
) -> Result<(), DesktopPlatformError<DisplayFailed>> {
    DesktopPlatform {
        display: StandInDisplay { served },
        services: vec![Box::new(service)],
    }
    .serve(links())
}

#[test]
fn serve_returns_once_the_display_returns_while_a_service_is_still_watching() {
    assert!(matches!(serve(Ok(()), WatchesForever), Ok(())));
}

#[test]
fn serve_returns_the_failure_of_the_display() {
    assert!(matches!(
        serve(Err(DisplayFailed), WatchesForever),
        Err(DesktopPlatformError::Display(DisplayFailed))
    ));
}

#[test]
fn serve_fails_when_the_desktop_services_cannot_start() {
    assert!(matches!(
        serve(Ok(()), PanicsWhileStarting),
        Err(DesktopPlatformError::StartServices(
            ServicesStartError::Panicked {
                value: PanicValue::Text(text)
            }
        )) if text == "the service panics while its watches start"
    ));
}

#[test]
fn a_store_without_the_desktop_layer_serves_with_no_services()
-> Result<(), DesktopPlatformError<DisplayFailed>> {
    let Connection { platform, .. } = DesktopPlatform::<StandInDisplay>::connect(Store::default())?;
    assert!(platform.services.is_empty());
    platform.serve(links())
}
