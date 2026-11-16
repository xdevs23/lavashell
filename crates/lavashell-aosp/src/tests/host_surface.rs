// SPDX-License-Identifier: GPL-3.0-only
use crate::host::surface::HostSurface;
use crate::host::{AospError, AospHost};
use crate::native_window::NativeWindow;
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::streams::AospStreams;
use lavashell_actor::{Merge, MergeEffect, Receiver, SystemClock, mailbox};
use lavashell_core::output::OutputRequest;
use lavashell_core::prelude::{DpExt, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{
    PlatformEvent, PlatformLinks, PlatformRequest, Reports, Role, StreamTask, SurfaceAppearance,
    SurfaceAttachment, SurfaceEvent, SurfaceRequest, SurfaceSpec, TimingEvent, WindowRole,
};
use std::sync::{Arc, mpsc};

const SURFACE_2: SurfaceId = SurfaceId::numbered(2);

const SURFACE: SurfaceId = SurfaceId::numbered(1);

#[derive(Debug, PartialEq)]
enum Sent {
    Surface(SurfaceId, SurfaceEvent),
    DisplayReady(SurfaceId),
    Attach(SurfaceId),
    Detach(SurfaceId),
    Unexpected,
}

impl Merge for Sent {
    type Key = ();

    fn effect(&self) -> MergeEffect<()> {
        MergeEffect::Appends
    }
}

fn reports() -> (Reports<NativeWindow, AospStreams>, Receiver<Sent>) {
    let (sender, sent) = mailbox();
    let reports = Reports {
        view: sender.outlet(|event: PlatformEvent| match event {
            PlatformEvent::Surface(surface, event) => Sent::Surface(surface, event),
            _ => Sent::Unexpected,
        }),
        scheduler: sender.outlet(|event: TimingEvent| match event {
            TimingEvent::DisplayReady(surface) => Sent::DisplayReady(surface),
            _ => Sent::Unexpected,
        }),
        renderer: sender.outlet(
            |attachment: SurfaceAttachment<NativeWindow>| match attachment {
                SurfaceAttachment::Attach(attach) => Sent::Attach(attach.surface),
                SurfaceAttachment::Control { message, .. } => match message {},
                SurfaceAttachment::Detach(surface) => Sent::Detach(surface),
            },
        ),
        streams: sender.outlet(|_: StreamTask<AospStreams>| Sent::Unexpected),
        clock: Arc::new(SystemClock::start()),
    };
    (reports, sent)
}

struct Host {
    surface: HostSurface,
    reports: Reports<NativeWindow, AospStreams>,
    sent: Receiver<Sent>,
}

impl Host {
    fn new() -> Self {
        let (reports, sent) = reports();
        Self {
            surface: HostSurface::default(),
            reports,
            sent,
        }
    }

    fn sent(&self) -> Result<Vec<Sent>, TestError> {
        Ok(self.sent.take_arrived()?)
    }

    fn execute(&mut self, request: SurfaceRequest) -> Result<(), AospError> {
        (self.surface).execute(PlatformRequest::Surface(request), &self.reports)
    }

    fn created(&mut self, window: NativeWindow) -> Result<(), AospError> {
        self.surface.created(window, &self.reports)
    }

    fn changed(&self) -> Result<(), AospError> {
        self.surface.changed(&self.reports)
    }

    fn destroyed(&mut self) -> mpsc::Receiver<()> {
        let (retired, waiter) = mpsc::sync_channel(1);
        self.surface.destroyed(retired, &self.reports);
        waiter
    }

    fn frame(&self) {
        self.surface.frame(&self.reports);
    }

    fn destroy_returned(self, waiter: &mpsc::Receiver<()>) -> bool {
        drop(self);
        waiter.recv().is_ok()
    }
}

fn create(surface: SurfaceId) -> SurfaceRequest {
    let appearance = SurfaceAppearance {
        transparent: false,
        output: OutputRequest::default(),
        hdr_metadata: None,
        background_blur: false,
    };
    let role = Role::Window(WindowRole {
        title: "app".to_owned(),
        app_id: "test.app".to_owned(),
        min_size: None,
        fullscreen: false,
        draws_decorations: false,
    });
    let spec = SurfaceSpec {
        size: Size::new(320.dp(), 240.dp()),
        appearance,
        role,
    };
    SurfaceRequest::Create {
        surface,
        spec: Box::new(spec),
    }
}

fn window(display: &DisplaySurface) -> NativeWindow {
    NativeWindow::new(display.window())
}

fn configured(surface: SurfaceId, width: i32, height: i32) -> Sent {
    let configured = SurfaceEvent::Configured {
        width: Some(width.dp()),
        height: Some(height.dp()),
        token: None,
    };
    Sent::Surface(surface, configured)
}

fn shown() -> Result<Host, TestError> {
    let mut host = Host::new();
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    host.execute(create(SURFACE))?;
    let _attached_and_configured = host.sent()?;
    Ok(host)
}

#[test]
fn a_surface_requested_before_the_host_surface_is_shown_with_it() -> Result<(), TestError> {
    let mut host = Host::new();
    host.execute(create(SURFACE))?;
    host.frame();
    assert_eq!(host.sent()?, []);
    host.created(window(&DisplaySurface::new(SIDE, SIDE / 2)))?;
    host.frame();
    let shown = [
        Sent::Attach(SURFACE),
        configured(SURFACE, SIDE, SIDE / 2),
        Sent::DisplayReady(SURFACE),
    ];
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_surface_requested_while_the_host_surface_exists_is_shown_at_once() -> Result<(), TestError> {
    let mut host = Host::new();
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    assert_eq!(host.sent()?, []);
    host.execute(create(SURFACE))?;
    let shown = [Sent::Attach(SURFACE), configured(SURFACE, SIDE, SIDE)];
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_host_surface_without_a_size_is_configured_once_it_has_one() -> Result<(), TestError> {
    let mut host = Host::new();
    let display = DisplaySurface::new(0, 0);
    host.created(window(&display))?;
    host.changed()?;
    host.execute(create(SURFACE))?;
    assert_eq!(host.sent()?, [Sent::Attach(SURFACE)]);
    display.resize(SIDE, SIDE / 2);
    host.changed()?;
    assert_eq!(host.sent()?, [configured(SURFACE, SIDE, SIDE / 2)]);
    Ok(())
}

#[test]
fn a_destroy_returns_at_once_while_no_surface_is_shown() -> Result<(), TestError> {
    let mut idle = Host::new();
    idle.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    let waiter = idle.destroyed();
    assert!(idle.destroy_returned(&waiter));

    let mut requested = Host::new();
    requested.execute(create(SURFACE))?;
    let waiter = requested.destroyed();
    assert_eq!(requested.sent()?, []);
    assert!(requested.destroy_returned(&waiter));
    Ok(())
}

#[test]
fn a_destroy_of_a_shown_surface_closes_it_and_returns_after_its_release() -> Result<(), TestError> {
    let mut unreleased = shown()?;
    let waiter = unreleased.destroyed();
    unreleased.execute(SurfaceRequest::Destroy(SURFACE))?;
    assert!(!unreleased.destroy_returned(&waiter));

    let mut host = shown()?;
    let waiter = host.destroyed();
    host.frame();
    host.execute(SurfaceRequest::Destroy(SURFACE))?;
    host.execute(SurfaceRequest::Release(SURFACE))?;
    let closed = [
        Sent::Surface(SURFACE, SurfaceEvent::Closed),
        Sent::Detach(SURFACE),
    ];
    assert_eq!(host.sent()?, closed);
    assert!(host.destroy_returned(&waiter));
    Ok(())
}

#[test]
fn a_destroy_while_the_surface_closes_returns_after_its_release() -> Result<(), TestError> {
    let mut unreleased = shown()?;
    unreleased.execute(SurfaceRequest::Destroy(SURFACE))?;
    let waiter = unreleased.destroyed();
    assert!(!unreleased.destroy_returned(&waiter));

    let mut host = shown()?;
    host.execute(SurfaceRequest::Destroy(SURFACE))?;
    let waiter = host.destroyed();
    host.execute(SurfaceRequest::Release(SURFACE))?;
    assert_eq!(host.sent()?, [Sent::Detach(SURFACE)]);
    assert!(host.destroy_returned(&waiter));
    Ok(())
}

#[test]
fn a_surface_created_again_while_the_host_surface_lives_is_shown_again() -> Result<(), TestError> {
    let mut host = shown()?;
    host.execute(SurfaceRequest::Destroy(SURFACE))?;
    host.execute(SurfaceRequest::Release(SURFACE))?;
    let again = SurfaceId::numbered(2);
    host.execute(create(again))?;
    let shown = [
        Sent::Detach(SURFACE),
        Sent::Attach(again),
        configured(again, SIDE, SIDE),
    ];
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_second_surface_or_a_second_host_surface_fails() -> Result<(), TestError> {
    let mut host = shown()?;
    assert!(matches!(
        host.execute(create(SurfaceId::numbered(2))),
        Err(AospError::SecondSurface {
            open: SURFACE,
            requested: SURFACE_2
        })
    ));
    assert!(matches!(
        host.created(window(&DisplaySurface::new(SIDE, SIDE))),
        Err(AospError::SecondWindow)
    ));
    Ok(())
}

#[test]
fn a_destroy_or_a_release_of_another_surface_fails() -> Result<(), TestError> {
    let mut idle = Host::new();
    assert!(matches!(
        idle.execute(SurfaceRequest::Destroy(SURFACE)),
        Err(AospError::UnknownSurface {
            open: None,
            requested: SURFACE
        })
    ));

    let mut host = shown()?;
    assert!(matches!(
        host.execute(SurfaceRequest::Destroy(SurfaceId::numbered(2))),
        Err(AospError::UnknownSurface {
            open: Some(SURFACE),
            requested: SURFACE_2
        })
    ));
    assert!(matches!(
        host.execute(SurfaceRequest::Release(SurfaceId::numbered(2))),
        Err(AospError::UnknownSurface {
            open: Some(SURFACE),
            requested: SURFACE_2
        })
    ));
    assert_eq!(host.sent()?, []);
    Ok(())
}

#[test]
fn a_run_inside_serve_reports_the_host_and_ends_with_its_requests() -> Result<(), TestError> {
    let host = AospHost::new();
    host.surface_created(window(&DisplaySurface::new(SIDE, SIDE)));
    host.frame();
    let (requests, received) = mailbox();
    requests.push(PlatformRequest::Surface(create(SURFACE)))?;
    drop(requests);
    let (reports, sent) = reports();
    host.clone().serve(|link| {
        link.serve(PlatformLinks {
            reports,
            requests: received,
        })
    })??;
    let shown = [Sent::Attach(SURFACE), configured(SURFACE, SIDE, SIDE)];
    assert_eq!(sent.take_arrived()?, shown);
    assert!(matches!(
        host.serve(|_link| ()),
        Err(AospError::HostAlreadyServed)
    ));
    Ok(())
}
