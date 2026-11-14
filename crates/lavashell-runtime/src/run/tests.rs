// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::renderer_actor::RendererError;
use crate::test_error::TestResult;
use crate::test_host::{
    StandIn, StandInError, closed, empty_view, end_on_marker, host, run_scripted, run_unreachable,
    runtime_setup, surface_requests, window,
};
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, PresentationFlags};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::DpExt;
use lavashell_platform::{PlatformEvent, SurfaceEvent, SurfaceRequest, TimingEvent};
use lavashell_renderer::{DrawError, FrameError, StateError};
use std::panic::resume_unwind;
use std::sync::mpsc;

const WINDOW: SurfaceId = SurfaceId(0);

fn configured() -> PlatformEvent {
    let length = Some(100.dp());
    PlatformEvent::Surface(
        WINDOW,
        SurfaceEvent::Configured {
            width: length,
            height: length,
            token: None,
        },
    )
}

#[test]
fn a_host_that_cannot_be_reached_ends_the_run_with_the_error_of_the_platform() {
    let outcome = run_unreachable(runtime_setup(vec![window("window")]));
    assert!(matches!(
        outcome,
        Err(RunError::Platform(StandInError::Unreachable))
    ));
}

#[test]
fn a_panel_on_a_platform_without_panels_ends_the_run_before_the_platform_serves() {
    let panel = SurfaceSetup::Panel(LayerSurface::new("bar"), empty_view());
    let (entered, entries) = mpsc::channel();
    let run = run_scripted(host([]), [], runtime_setup(vec![panel]), move |wire| {
        let _test_gone = entered.send(());
        wire.finish()
    });
    assert!(matches!(
        &run.outcome,
        Err(RunError::Unsupported(missing)) if *missing == BTreeSet::from([Capability::Panels])
    ));
    assert!(entries.recv().is_err());
}

#[test]
fn an_app_without_a_surface_fails_before_the_platform_connects() {
    let app = App::new(lavashell_test_fixtures::facts());
    assert!(matches!(run::<StandIn>(app), Err(RunError::NoSurfaces)));
}

#[test]
fn a_run_ends_once_its_last_surface_closed_and_the_platform_returned() -> TestResult {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |wire| {
        wire.event(closed(WINDOW))?;
        wire.finish()
    });
    let requests = run.requests()?;
    assert!(matches!(
        surface_requests(&requests).as_slice(),
        [
            SurfaceRequest::Create {
                surface: WINDOW,
                ..
            },
            SurfaceRequest::Destroy(WINDOW)
        ]
    ));
    Ok(())
}

#[test]
fn a_destroyed_surface_is_released_once_the_platform_detached_it_from_the_renderer() -> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    setup.panels_per_output = Some(PanelsPerOutput::new(|_| Vec::new()));
    end_on_marker(&mut setup);
    let run = run_scripted(host([]), Capability::ALL, setup, |mut wire| {
        wire.event(closed(WINDOW))?;
        wire.await_surface_request(&SurfaceRequest::Destroy(WINDOW))?;
        wire.detach(WINDOW)?;
        wire.await_surface_request(&SurfaceRequest::Release(WINDOW))?;
        wire.end_marker()?;
        wire.finish()
    });
    let requests = run.requests()?;
    assert!(matches!(
        surface_requests(&requests).as_slice(),
        [
            SurfaceRequest::Create {
                surface: WINDOW,
                ..
            },
            SurfaceRequest::Destroy(WINDOW),
            SurfaceRequest::Release(WINDOW)
        ]
    ));
    Ok(())
}

#[test]
fn an_error_of_the_platform_after_the_run_ended_is_returned() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |wire| {
        wire.event(closed(WINDOW))?;
        wire.finish()?;
        Err(StandInError::Failed)
    });
    assert!(matches!(
        run.outcome,
        Err(RunError::Platform(StandInError::Failed))
    ));
}

#[test]
fn an_error_of_the_platform_during_the_run_replaces_the_stop_of_the_views() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |_wire| {
        Err(StandInError::Failed)
    });
    assert!(matches!(
        run.outcome,
        Err(RunError::Platform(StandInError::Failed))
    ));
}

#[test]
fn a_platform_that_returns_during_the_run_stops_the_views() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |_wire| Ok(()));
    assert!(matches!(
        run.outcome,
        Err(RunError::View(ViewError::Stopped(Stopped::Platform)))
    ));
}

#[test]
fn a_platform_that_panics_during_the_run_ends_it_with_a_panic_of_the_platform() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |_wire| {
        resume_unwind(Box::new("the platform crashed"))
    });
    assert!(matches!(run.outcome, Err(RunError::PlatformPanicked)));
}

#[test]
fn a_platform_that_panics_after_the_run_ended_is_reported() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |wire| {
        wire.event(closed(WINDOW))?;
        wire.finish()?;
        resume_unwind(Box::new("the platform crashed"))
    });
    assert!(matches!(run.outcome, Err(RunError::PlatformPanicked)));
}

#[test]
fn a_renderer_that_fails_ends_the_run_with_its_failure() {
    let setup = runtime_setup(vec![window("window")]);
    let run = run_scripted(host([]), Capability::ALL, setup, |wire| {
        wire.event(configured())?;
        wire.finish()
    });
    assert!(matches!(
        run.outcome,
        Err(RunError::RunRenderer(JoinError::Failed(
            RendererError::Draw(DrawError::Frame(FrameError::State(
                StateError::UnknownSurface(WINDOW)
            )))
        )))
    ));
}

#[test]
fn frame_reports_reach_the_frame_observer_on_the_thread_of_the_views() -> TestResult {
    let (seen, reports) = mpsc::channel();
    let mut setup = runtime_setup(vec![window("window")]);
    setup.observers.frame_report = Some(Box::new(move |report: FrameReport| {
        let _observer_gone = seen.send((thread::current().id(), report.surface, report.stage));
    }));
    let run = run_scripted(host([]), Capability::ALL, setup, |wire| {
        let frame = FrameTime::default();
        wire.timing(TimingEvent::Discarded {
            surface: WINDOW,
            frame,
        })?;
        wire.timing(TimingEvent::Presented {
            surface: WINDOW,
            frame,
            time: None,
            refresh: None,
            flags: PresentationFlags::default(),
        })?;
        wire.event(closed(WINDOW))?;
        wire.finish()
    });
    run.outcome?;
    let views = thread::current().id();
    let expected = [
        (views, WINDOW, FrameStage::Discarded),
        (views, WINDOW, FrameStage::Presented),
    ];
    assert_eq!(reports.iter().collect::<Vec<_>>(), expected);
    Ok(())
}
