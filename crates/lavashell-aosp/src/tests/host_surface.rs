// SPDX-License-Identifier: GPL-3.0-only
use super::motion::{finger, hovering, mouse, touch};
use crate::host::{AospError, AospHost, AospReports, HostMessage, HostState};
use crate::input::{MOUSE, MotionEvent, MouseAction, TouchAction};
use crate::layout_size::LayoutSize;
use crate::native_window::NativeWindow;
use crate::output::{DISPLAY, HostDisplay, report_added};
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::settings::HostConfiguration;
use crate::streams::AospStreams;
use icu_locale_core::langid;
use lavashell_actor::{Merge, MergeEffect, Receiver, Sender, SystemClock, mailbox};
use lavashell_core::output::{MonitorInfo, OutputRequest};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_core::prelude::{Density, DpExt, PixelScale, Point, PxExt, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::{PointerKind, TouchId};
use lavashell_core::view::{Cursor, TouchSample};
use lavashell_core::window::{Edge, Edges, WindowCapabilities, WindowState};
use lavashell_platform::{
    DeviceEvent, InputRequest, OutputEvent, OutputId, PlatformEvent, PlatformLinks,
    PlatformRequest, Reports, Role, SettingsEvent, StreamTask, SurfaceAppearance,
    SurfaceAttachment, SurfaceEvent, SurfaceRequest, SurfaceSpec, TimingEvent, WindowRole,
};
use std::sync::{Arc, mpsc};
use std::time::Duration;

mod host_display;
mod layout_size;

const SURFACE_2: SurfaceId = SurfaceId::numbered(2);

const SURFACE: SurfaceId = SurfaceId::numbered(1);

const REFRESH: Duration = Duration::from_millis(16);

#[derive(Debug, PartialEq)]
enum Sent {
    Surface(SurfaceId, SurfaceEvent),
    Output(OutputEvent),
    Settings(SettingsEvent),
    Device(DeviceEvent),
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

fn reports() -> (AospReports, Receiver<Sent>) {
    let (sender, sent) = mailbox();
    let reports = Reports {
        view: sender.outlet(|event: PlatformEvent| match event {
            PlatformEvent::Surface(surface, event) => Sent::Surface(surface, event),
            PlatformEvent::Output(event) => Sent::Output(event),
            PlatformEvent::Settings(event) => Sent::Settings(event),
            PlatformEvent::Device(event) => Sent::Device(event),
            _ => Sent::Unexpected,
        }),
        scheduler: sender.outlet(|event: TimingEvent| match event {
            TimingEvent::DisplayReady(surface) => Sent::DisplayReady(surface),
            _ => Sent::Unexpected,
        }),
        renderer: sender.outlet(
            |attachment: SurfaceAttachment<NativeWindow, _>| match attachment {
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
    state: HostState,
    requests: Sender<PlatformRequest>,
    received: Receiver<PlatformRequest>,
    reports: AospReports,
    sent: Receiver<Sent>,
}

impl Host {
    fn new(display: HostDisplay) -> Self {
        let (reports, sent) = reports();
        let (requests, received) = mailbox();
        report_added(display, &reports);
        Self {
            state: HostState::new(display.density),
            requests,
            received,
            reports,
            sent,
        }
    }

    fn sent(&self) -> Result<Vec<Sent>, TestError> {
        Ok(self.sent.take_arrived()?)
    }

    fn handle(&mut self, message: HostMessage) -> Result<(), AospError> {
        let _requests_open_while_the_host_lives =
            self.state.handle(message, &self.received, &self.reports)?;
        Ok(())
    }

    fn request(&mut self, request: PlatformRequest) -> Result<(), AospError> {
        let _received_by_this_host = self.requests.push(request);
        self.handle(HostMessage::Requests)
    }

    fn execute(&mut self, request: SurfaceRequest) -> Result<(), AospError> {
        self.request(PlatformRequest::Surface(request))
    }

    fn created(&mut self, window: NativeWindow) -> Result<(), AospError> {
        self.handle(HostMessage::SurfaceCreated(window))
    }

    fn changed(&mut self) -> Result<(), AospError> {
        self.handle(HostMessage::SurfaceChanged)
    }

    fn set_layout_size(&mut self, size: Option<LayoutSize>) -> Result<(), AospError> {
        self.handle(HostMessage::LayoutSize(size))
    }

    fn display_changed(&mut self, display: HostDisplay) -> Result<(), AospError> {
        self.handle(HostMessage::DisplayChanged(display))
    }

    fn destroyed(&mut self) -> Result<mpsc::Receiver<()>, AospError> {
        let (retired, waiter) = mpsc::sync_channel(1);
        self.handle(HostMessage::SurfaceDestroyed(retired))?;
        Ok(waiter)
    }

    fn frame(&mut self) -> Result<(), AospError> {
        self.handle(HostMessage::Frame)
    }

    fn motion(&mut self, event: MotionEvent) -> Result<(), AospError> {
        self.handle(HostMessage::Motion(event))
    }

    fn destroy_returned(self, waiter: &mpsc::Receiver<()>) -> bool {
        drop(self);
        waiter.recv().is_ok()
    }
}

fn create(surface: SurfaceId) -> SurfaceRequest {
    create_as(
        surface,
        Role::Window(WindowRole {
            title: "app".to_owned(),
            app_id: "test.app".to_owned(),
            min_size: None,
            fullscreen: false,
            draws_decorations: false,
        }),
    )
}

fn create_as(surface: SurfaceId, role: Role) -> SurfaceRequest {
    let appearance = SurfaceAppearance {
        transparent: false,
        output: OutputRequest::default(),
        hdr_metadata: None,
        background_blur: false,
    };
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

fn display(density: Density) -> HostDisplay {
    HostDisplay {
        width: SIDE.px(),
        height: SIDE.px(),
        density,
        refresh: Some(REFRESH),
    }
}

fn two_pixels_per_dp() -> Result<Density, TestError> {
    Ok(Density::from_ratio(2.0, 1.0)?)
}

fn added(side: i32) -> Vec<Sent> {
    let mut reported = vec![Sent::Output(OutputEvent::Added(DISPLAY))];
    reported.extend(updated(side));
    reported
}

fn updated(side: i32) -> [Sent; 2] {
    let monitor = MonitorInfo {
        name: None,
        size: Size::new(side.dp(), side.dp()),
        physical_size: None,
    };
    [
        Sent::Output(OutputEvent::Updated(DISPLAY, monitor)),
        Sent::Output(OutputEvent::Refresh(DISPLAY, Some(REFRESH))),
    ]
}

fn displayed(display: HostDisplay) -> Result<Host, TestError> {
    let host = Host::new(display);
    let _output_reported = host.sent()?;
    Ok(host)
}

fn configuration(color_scheme: ColorScheme) -> HostConfiguration {
    HostConfiguration {
        color_scheme,
        locales: vec![langid!("en")],
    }
}

fn settings(color_scheme: ColorScheme) -> [Sent; 2] {
    let appearance = Appearance {
        color_scheme: Some(color_scheme),
        ..Appearance::default()
    };
    [
        Sent::Settings(SettingsEvent::Appearance(appearance)),
        Sent::Settings(SettingsEvent::Locales(vec![langid!("en")])),
    ]
}

fn configured(surface: SurfaceId, width: i32, height: i32) -> Sent {
    let display_filling = WindowState {
        maximized: false,
        fullscreen: false,
        tiled: Edges::new([Edge::Top, Edge::Right, Edge::Bottom, Edge::Left]),
        suspended: false,
        capabilities: WindowCapabilities::default(),
    };
    let configured = SurfaceEvent::Configured {
        width: Some(width.dp()),
        height: Some(height.dp()),
        token: None,
        window: Some(display_filling),
    };
    Sent::Surface(surface, configured)
}

fn laid_out(surface: SurfaceId, scale: PixelScale, width: i32, height: i32) -> [Sent; 2] {
    let scaled = Sent::Surface(surface, SurfaceEvent::ScaleChanged(scale));
    [scaled, configured(surface, width, height)]
}

fn showing(
    surface: SurfaceId,
    density: Density,
    layout: impl IntoIterator<Item = Sent>,
) -> Vec<Sent> {
    let attached = [
        Sent::Attach(surface),
        Sent::Surface(surface, SurfaceEvent::DensityChanged(density)),
    ];
    let entered = Sent::Output(OutputEvent::Entered(DISPLAY, surface));
    (attached.into_iter().chain(layout).chain([entered])).collect()
}

fn touched(x: f64, y: f64) -> TouchSample {
    TouchSample {
        id: TouchId { seat: 0, point: 0 },
        at: Point::new(x.dp(), y.dp()),
        kind: PointerKind::Finger,
    }
}

fn touch_down(sample: TouchSample) -> Sent {
    Sent::Device(DeviceEvent::TouchDown {
        surface: SURFACE,
        sample,
    })
}

fn shown() -> Result<Host, TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    host.execute(create(SURFACE))?;
    let _attached_and_configured = host.sent()?;
    Ok(host)
}

#[test]
fn a_surface_requested_before_the_host_surface_is_shown_with_it() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.execute(create(SURFACE))?;
    host.frame()?;
    assert_eq!(host.sent()?, []);
    host.created(window(&DisplaySurface::new(SIDE, SIDE / 2)))?;
    host.frame()?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, SIDE, SIDE / 2);
    let mut shown = showing(SURFACE, Density::ONE, square);
    shown.push(Sent::DisplayReady(SURFACE));
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_surface_requested_while_the_host_surface_exists_is_shown_at_once() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    assert_eq!(host.sent()?, []);
    host.execute(create(SURFACE))?;
    let square = laid_out(SURFACE, PixelScale::SQUARE, SIDE, SIDE);
    assert_eq!(host.sent()?, showing(SURFACE, Density::ONE, square));
    Ok(())
}

#[test]
fn a_surface_that_is_not_a_window_is_configured_without_a_window_state() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    host.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    let lock = Role::Lock {
        output: OutputId(0),
    };
    host.execute(create_as(SURFACE, lock))?;
    let configured = SurfaceEvent::Configured {
        width: Some(SIDE.dp()),
        height: Some(SIDE.dp()),
        token: None,
        window: None,
    };
    let square = [
        Sent::Surface(SURFACE, SurfaceEvent::ScaleChanged(PixelScale::SQUARE)),
        Sent::Surface(SURFACE, configured),
    ];
    assert_eq!(host.sent()?, showing(SURFACE, Density::ONE, square));
    Ok(())
}

#[test]
fn a_destroy_returns_at_once_while_no_surface_is_shown() -> Result<(), TestError> {
    let mut idle = displayed(display(Density::ONE))?;
    idle.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    let waiter = idle.destroyed()?;
    assert!(idle.destroy_returned(&waiter));

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    let waiter = requested.destroyed()?;
    assert_eq!(requested.sent()?, []);
    assert!(requested.destroy_returned(&waiter));
    Ok(())
}

#[test]
fn a_destroy_of_a_shown_surface_closes_it_and_returns_after_its_release() -> Result<(), TestError> {
    let mut unreleased = shown()?;
    let waiter = unreleased.destroyed()?;
    unreleased.execute(SurfaceRequest::Destroy(SURFACE))?;
    assert!(!unreleased.destroy_returned(&waiter));

    let mut host = shown()?;
    let waiter = host.destroyed()?;
    host.frame()?;
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
    let waiter = unreleased.destroyed()?;
    assert!(!unreleased.destroy_returned(&waiter));

    let mut host = shown()?;
    host.execute(SurfaceRequest::Destroy(SURFACE))?;
    let waiter = host.destroyed()?;
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
    let mut shown = vec![Sent::Detach(SURFACE)];
    let square = laid_out(again, PixelScale::SQUARE, SIDE, SIDE);
    shown.extend(showing(again, Density::ONE, square));
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_motion_produces_device_events_only_on_a_shown_surface() -> Result<(), TestError> {
    let corner = || touch(0, TouchAction::Down(finger(0, 0.0, 0.0)));
    let mut idle = displayed(display(Density::ONE))?;
    idle.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    idle.motion(corner())?;
    assert_eq!(idle.sent()?, []);

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    requested.motion(corner())?;
    assert_eq!(requested.sent()?, []);

    let mut host = shown()?;
    host.motion(corner())?;
    let sent = host.sent()?;
    assert!(matches!(
        sent.as_slice(),
        [Sent::Device(DeviceEvent::TouchDown {
            surface: SURFACE,
            ..
        })]
    ));
    Ok(())
}

#[test]
fn a_cursor_request_for_a_mouse_over_the_surface_lets_the_run_go_on() -> Result<(), TestError> {
    let mut host = shown()?;
    host.motion(mouse(MouseAction::Move(hovering(10.0, 10.0))))?;
    let cursor = InputRequest::SetCursor(MOUSE, Cursor::Pointer);
    host.request(PlatformRequest::Input(cursor))?;
    host.frame()?;
    let entered = DeviceEvent::PointerEnter {
        pointer: MOUSE,
        surface: SURFACE,
        at: Point::new(10.dp(), 10.dp()),
    };
    assert_eq!(
        host.sent()?,
        [Sent::Device(entered), Sent::DisplayReady(SURFACE)]
    );
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
    let mut idle = displayed(display(Density::ONE))?;
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
    let host = AospHost::new(display(Density::ONE), configuration(ColorScheme::Light));
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
    let mut reported = added(SIDE);
    reported.extend(settings(ColorScheme::Light));
    let square = laid_out(SURFACE, PixelScale::SQUARE, SIDE, SIDE);
    reported.extend(showing(SURFACE, Density::ONE, square));
    assert_eq!(sent.take_arrived()?, reported);
    assert!(matches!(
        host.serve(|_link| ()),
        Err(AospError::HostAlreadyServed)
    ));
    Ok(())
}

#[test]
fn a_configuration_change_reaches_the_runtime_as_settings() -> Result<(), TestError> {
    let host = AospHost::new(display(Density::ONE), configuration(ColorScheme::Light));
    host.configuration_changed(configuration(ColorScheme::Dark));
    let (requests, received) = mailbox();
    drop(requests);
    let (reports, sent) = reports();
    host.serve(|link| {
        link.serve(PlatformLinks {
            reports,
            requests: received,
        })
    })??;
    let mut reported = added(SIDE);
    reported.extend(settings(ColorScheme::Light));
    reported.extend(settings(ColorScheme::Dark));
    assert_eq!(sent.take_arrived()?, reported);
    Ok(())
}
