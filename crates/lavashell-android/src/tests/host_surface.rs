// SPDX-License-Identifier: GPL-3.0-only
use super::motion::{finger, hovering, mouse, touch};
use crate::host::surface::HostSurface;
use crate::host::{AospError, AospHost, take_served_link};
use crate::input::{MOUSE, MotionEvent, MotionTranslator, MouseAction, TouchAction};
use crate::native_window::NativeWindow;
use crate::output::{DISPLAY, HostDisplay, HostOutput};
use crate::platform::{DisplaySurface, SIDE, TestError};
use crate::settings::HostConfiguration;
use crate::streams::AospStreams;
use icu_locale_core::langid;
use lavashell_actor::{Effect, Merge, Receiver, SystemClock, mailbox};
use lavashell_core::output::{MonitorInfo, OutputRequest};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences;
use lavashell_core::prelude::{Density, DpExt, Point, PxExt, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::Cursor;
use lavashell_platform::{
    Appearance, DeviceEvent, InputRequest, OutputEvent, PlatformEvent, PlatformLinks,
    PlatformRequest, Reports, Role, SettingsEvent, StreamTask, SurfaceAttachment, SurfaceEvent,
    SurfaceRequest, SurfaceSpec, TimingEvent, WindowRole,
};
use lavashell_renderer::{PresentationPolicy, SurfaceTiming};
use std::sync::{Arc, mpsc};
use std::time::Duration;

mod host_display;

const SURFACE: SurfaceId = SurfaceId(1);
const REFRESH: Duration = Duration::from_millis(16);

/// What the platform sent to the actors of the runtime.
#[derive(Debug, PartialEq)]
enum Sent {
    Surface(SurfaceId, SurfaceEvent),
    Output(OutputEvent),
    Settings(SettingsEvent),
    Device(DeviceEvent),
    DisplayReady(SurfaceId),
    Attach(SurfaceId),
    Detach(SurfaceId),
    /// A report that an AOSP host does not send.
    Unexpected,
}

impl Merge for Sent {
    type Key = ();

    fn effect(&self) -> Effect<()> {
        Effect::BuildsOn
    }
}

/// Reports whose items arrive in order in the returned receiver.
fn reports() -> (Reports<NativeWindow, AospStreams>, Receiver<Sent>) {
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
            |attachment: SurfaceAttachment<NativeWindow>| match attachment {
                SurfaceAttachment::Attach { attach, .. } => Sent::Attach(attach.surface),
                SurfaceAttachment::Control { message, .. } => match message {},
                SurfaceAttachment::Detach(surface) => Sent::Detach(surface),
            },
        ),
        streams: sender.outlet(|_: StreamTask<AospStreams>| Sent::Unexpected),
        clock: Arc::new(SystemClock::start()),
    };
    (reports, sent)
}

/// The display and the surface of an AOSP host, and the translator of motion events. Reports from all of them arrive
/// in `sent`.
struct Host {
    output: HostOutput,
    surface: HostSurface,
    translator: MotionTranslator,
    reports: Reports<NativeWindow, AospStreams>,
    sent: Receiver<Sent>,
}

impl Host {
    /// A host on `display`, with the reports that add the display not yet drained from `sent`.
    fn new(display: HostDisplay) -> Self {
        let (reports, sent) = reports();
        Self {
            output: HostOutput::added(display, &reports),
            surface: HostSurface::default(),
            translator: MotionTranslator::default(),
            reports,
            sent,
        }
    }

    /// The reports sent since the last call.
    fn sent(&self) -> Result<Vec<Sent>, TestError> {
        Ok(self.sent.take_arrived()?)
    }

    fn execute(&mut self, request: SurfaceRequest) -> Result<(), AospError> {
        let request = PlatformRequest::Surface(request);
        (self.surface).execute(request, self.output.density(), &self.reports)
    }

    fn created(&mut self, window: NativeWindow) -> Result<(), AospError> {
        (self.surface).created(window, self.output.density(), &self.reports)
    }

    fn changed(&mut self) -> Result<(), AospError> {
        self.surface.changed(&self.reports)
    }

    fn display_changed(&mut self, display: HostDisplay) {
        if let Some(density) = self.output.changed(display, &self.reports) {
            self.surface.density_changed(density, &self.reports);
        }
    }

    /// Destroys the Surface of the host, and returns the receiver on which the platform lets the destroy return.
    fn destroyed(&mut self) -> mpsc::Receiver<()> {
        let (retired, waiter) = mpsc::sync_channel(1);
        self.surface.destroyed(retired, &self.reports);
        waiter
    }

    fn frame(&self) {
        self.surface.frame(&self.reports);
    }

    fn motion(&mut self, event: MotionEvent) {
        (self.translator).report(event, self.surface.shown(), &self.reports);
    }

    /// Drops the host, and returns whether the platform let the destroy of `waiter` return before.
    fn destroy_returned(self, waiter: &mpsc::Receiver<()>) -> bool {
        drop(self);
        waiter.recv().is_ok()
    }
}

fn create(surface: SurfaceId) -> SurfaceRequest {
    let appearance = Appearance {
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
        gpu_budget: None,
    }
}

fn window(display: &DisplaySurface) -> NativeWindow {
    let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
    NativeWindow::new(display.window(), timing)
}

/// A display of `SIDE` × `SIDE` pixels at `density`.
fn display(density: Density) -> HostDisplay {
    HostDisplay {
        width: SIDE.px(),
        height: SIDE.px(),
        density,
        refresh: Some(REFRESH),
    }
}

/// The reports of the first display, at `side` × `side` dp.
fn added(side: i32) -> [Sent; 3] {
    let monitor = MonitorInfo {
        name: None,
        size: Size::new(side.dp(), side.dp()),
        physical_size: None,
    };
    [
        Sent::Output(OutputEvent::Added(DISPLAY)),
        Sent::Output(OutputEvent::Updated(DISPLAY, monitor)),
        Sent::Output(OutputEvent::Refresh(DISPLAY, Some(REFRESH))),
    ]
}

/// A host on `display`, after the display reports were drained from `sent`.
fn displayed(display: HostDisplay) -> Result<Host, TestError> {
    let host = Host::new(display);
    let _output_reported = host.sent()?;
    Ok(host)
}

/// A configuration of `color_scheme` in English.
fn configuration(color_scheme: ColorScheme) -> HostConfiguration {
    HostConfiguration {
        color_scheme,
        locales: vec![langid!("en")],
    }
}

/// The reports sent for `configuration(color_scheme)`.
fn settings(color_scheme: ColorScheme) -> [Sent; 2] {
    let appearance = preferences::Appearance {
        color_scheme: Some(color_scheme),
        ..preferences::Appearance::default()
    };
    [
        Sent::Settings(SettingsEvent::Appearance(appearance)),
        Sent::Settings(SettingsEvent::Locales(vec![langid!("en")])),
    ]
}

/// The reports sent when `surface` is configured to `width` × `height` dp.
fn configured(surface: SurfaceId, width: i32, height: i32) -> Sent {
    let configured = SurfaceEvent::Configured {
        width: Some(width.dp()),
        height: Some(height.dp()),
        token: None,
    };
    Sent::Surface(surface, configured)
}

/// The reports sent when `surface` is shown, configured to `width` × `height` dp at `density`.
fn showing(surface: SurfaceId, width: i32, height: i32, density: Density) -> Vec<Sent> {
    vec![
        Sent::Attach(surface),
        Sent::Surface(surface, SurfaceEvent::DensityChanged(density)),
        configured(surface, width, height),
        Sent::Output(OutputEvent::Entered(DISPLAY, surface)),
    ]
}

/// A host that shows `SURFACE` in a Surface of `SIDE` × `SIDE` pixels on a display of one dp per pixel, after the show
/// reports were drained from `sent`.
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
    host.frame();
    assert_eq!(host.sent()?, []);
    host.created(window(&DisplaySurface::new(SIDE, SIDE / 2)))?;
    host.frame();
    let mut shown = showing(SURFACE, SIDE, SIDE / 2, Density::ONE);
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
    assert_eq!(host.sent()?, showing(SURFACE, SIDE, SIDE, Density::ONE));
    Ok(())
}

#[test]
fn a_host_surface_without_a_size_is_configured_once_it_has_one() -> Result<(), TestError> {
    let mut host = displayed(display(Density::ONE))?;
    let surface = DisplaySurface::new(0, 0);
    host.created(window(&surface))?;
    host.changed()?;
    host.execute(create(SURFACE))?;
    let unconfigured = [
        Sent::Attach(SURFACE),
        Sent::Surface(SURFACE, SurfaceEvent::DensityChanged(Density::ONE)),
        Sent::Output(OutputEvent::Entered(DISPLAY, SURFACE)),
    ];
    assert_eq!(host.sent()?, unconfigured);
    surface.resize(SIDE, SIDE / 2);
    host.changed()?;
    assert_eq!(host.sent()?, [configured(SURFACE, SIDE, SIDE / 2)]);
    Ok(())
}

#[test]
fn a_destroy_returns_at_once_while_no_surface_is_shown() -> Result<(), TestError> {
    let mut idle = displayed(display(Density::ONE))?;
    idle.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    let waiter = idle.destroyed();
    assert!(idle.destroy_returned(&waiter));

    let mut requested = displayed(display(Density::ONE))?;
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
    let again = SurfaceId(2);
    host.execute(create(again))?;
    let mut shown = vec![Sent::Detach(SURFACE)];
    shown.extend(showing(again, SIDE, SIDE, Density::ONE));
    assert_eq!(host.sent()?, shown);
    Ok(())
}

#[test]
fn a_motion_produces_device_events_only_on_a_shown_surface() -> Result<(), TestError> {
    let corner = || touch(0, TouchAction::Down(finger(0, 0.0, 0.0)));
    let mut idle = displayed(display(Density::ONE))?;
    idle.created(window(&DisplaySurface::new(SIDE, SIDE)))?;
    idle.motion(corner());
    assert_eq!(idle.sent()?, []);

    let mut requested = displayed(display(Density::ONE))?;
    requested.execute(create(SURFACE))?;
    requested.motion(corner());
    assert_eq!(requested.sent()?, []);

    let mut host = shown()?;
    host.motion(corner());
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
    host.motion(mouse(MouseAction::Move(hovering(10.0, 10.0))));
    let cursor = InputRequest::SetCursor(MOUSE, Cursor::Pointer);
    let request = PlatformRequest::Input(cursor);
    (host.surface).execute(request, host.output.density(), &host.reports)?;
    host.frame();
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
        host.execute(create(SurfaceId(2))),
        Err(AospError::SecondSurface {
            open: SURFACE,
            requested: SurfaceId(2)
        })
    ));
    assert!(matches!(
        host.created(window(&DisplaySurface::new(SIDE, SIDE))),
        Err(AospError::SecondWindow)
    ));
    Ok(())
}

/// Serves the link that `AospHost::serve` set for this thread through `links`.
fn serve_link(links: PlatformLinks<NativeWindow, AospStreams>) -> Result<(), AospError> {
    let link = take_served_link().ok_or(AospError::Unserved)?;
    link.serve(links)
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
    host.clone().serve(|| {
        serve_link(PlatformLinks {
            reports,
            requests: received,
        })
    })?;
    let mut reported = Vec::from(added(SIDE));
    reported.extend(settings(ColorScheme::Light));
    reported.extend(showing(SURFACE, SIDE, SIDE, Density::ONE));
    assert_eq!(sent.take_arrived()?, reported);
    assert!(host.serve(take_served_link).is_none());
    AospHost::new(display(Density::ONE), configuration(ColorScheme::Light)).serve(|| ());
    assert!(take_served_link().is_none());
    Ok(())
}

#[test]
fn a_configuration_change_reaches_the_runtime_as_settings() -> Result<(), TestError> {
    let host = AospHost::new(display(Density::ONE), configuration(ColorScheme::Light));
    host.configuration_changed(configuration(ColorScheme::Dark));
    let (requests, received) = mailbox();
    drop(requests);
    let (reports, sent) = reports();
    host.serve(|| {
        serve_link(PlatformLinks {
            reports,
            requests: received,
        })
    })?;
    let mut reported = Vec::from(added(SIDE));
    reported.extend(settings(ColorScheme::Light));
    reported.extend(settings(ColorScheme::Dark));
    assert_eq!(sent.take_arrived()?, reported);
    Ok(())
}
