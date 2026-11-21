// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Outlet, SystemClock, mailbox};
use lavashell_desktop_settings::{SettingsReporter, SettingsWatch};
use lavashell_platform::{HostFacts, Reports, SurfaceControl};
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
    type Setup = Result<(), DisplayFailed>;
    type Error = DisplayFailed;
    type Backend = HeadlessBackend;
    type Control = NoControl;

    fn connect(served: Self::Setup) -> Result<Connection<Self>, DisplayFailed> {
        Ok(Connection {
            platform: Self { served },
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

impl DesktopSettingSource for WatchesForever {
    fn watch(self: Box<Self>, _report: SettingsReporter) -> SettingsWatch {
        Box::pin(future::pending())
    }
}

struct PanicsWhileStarting;

impl DesktopSettingSource for PanicsWhileStarting {
    fn watch(self: Box<Self>, _report: SettingsReporter) -> SettingsWatch {
        panic::resume_unwind(Box::new("the source panics while its watch starts"))
    }
}

fn serve(
    served: Result<(), DisplayFailed>,
    source: impl DesktopSettingSource + 'static,
) -> Result<(), DesktopPlatformError<DisplayFailed>> {
    let (_requests, requests) = mailbox();
    let links = PlatformLinks {
        reports: Reports {
            view: Outlet::new(|_| Ok(())),
            scheduler: Outlet::new(|_| Ok(())),
            renderer: Outlet::new(|_| Ok(())),
            streams: Outlet::new(|_| Ok(())),
            clock: Arc::new(SystemClock::start()),
        },
        requests,
    };
    DesktopPlatform {
        display: StandInDisplay { served },
        sources: vec![Box::new(source)],
    }
    .serve(links)
}

#[test]
fn serve_returns_once_the_display_returns_while_a_setting_is_still_watched() {
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
fn serve_fails_when_the_desktop_settings_cannot_start() {
    assert!(matches!(
        serve(Ok(()), PanicsWhileStarting),
        Err(DesktopPlatformError::StartSettings(
            SettingsStartError::Panicked
        ))
    ));
}
