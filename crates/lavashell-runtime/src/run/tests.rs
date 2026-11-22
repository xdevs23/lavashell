// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::renderer_actor::RendererError;
use crate::test_error::TestResult;
use crate::test_host::{
    Scenario, StandIn, StandInError, closed, empty_view, end_on_marker, host, run_scripted,
    run_scripted_on, run_unreachable, runtime_setup, surface_requests, window,
};
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, PresentationFlags};
use lavashell_core::geometry::ScaledViewport;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::DpExt;
use lavashell_platform::{
    OutputEvent, OutputId, PlatformEvent, SurfaceEvent, SurfaceRequest, TimingEvent,
};
use lavashell_vulkan_renderer::vulkan::{PendingScene, SceneSource, VulkanRenderer};
use lavashell_vulkan_renderer::{DrawError, FrameError, HeadlessBackend, StateError};
use std::cell::Cell;
use std::panic::resume_unwind;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

const WINDOW: SurfaceId = SurfaceId::numbered(0);
const OUTPUT: OutputId = OutputId(1);

fn configured() -> PlatformEvent {
    let length = Some(100.dp());
    PlatformEvent::Surface(
        WINDOW,
        SurfaceEvent::Configured {
            width: length,
            height: length,
            token: None,
            window: None,
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
    let app = App::new().facts(lavashell_test_fixtures::facts());
    assert!(matches!(
        run::<StandIn>(app, Scenario::Unreachable),
        Err(RunError::NoSurfaces)
    ));
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
    release_after_detach(Some(HeadlessBackend::without_surfaces()))
}

#[test]
fn a_destroyed_surface_is_released_once_the_platform_detached_it_without_a_renderer() -> TestResult
{
    release_after_detach(None)
}

fn release_after_detach(backend: Option<HeadlessBackend>) -> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    setup.panels_per_output = Some(PanelsPerOutput::new(|_| Vec::new()));
    end_on_marker(&mut setup);
    let run = run_scripted_on(backend, host([]), Capability::ALL, setup, |mut wire| {
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

struct EmptyScene;

impl SceneSource for EmptyScene {
    fn next_deadline(&self) -> Duration {
        Duration::ZERO
    }

    fn produce(
        &mut self,
        _: &mut VulkanRenderer,
        _: Duration,
        _: ScaledViewport,
    ) -> Result<Option<PendingScene>, lavashell_vulkan_renderer::Error> {
        Ok(None)
    }
}

#[test]
fn a_scene_that_the_app_sets_on_the_backend_starts_with_the_renderer() -> TestResult {
    let mut app = App::new().facts(lavashell_test_fixtures::facts());
    app.window(Window::new("window", "test.app"), |_| {})?;
    let (made, scenes) = mpsc::channel();
    let make_scene = move || -> Box<dyn SceneSource> {
        let _test_gone = made.send(());
        Box::new(EmptyScene)
    };
    let (log, _requests_kept_for_the_log) = mpsc::channel();
    let scenario = Scenario::Scripted {
        backend: Some(HeadlessBackend::without_surfaces()),
        host: host([]),
        offered: Capability::ALL.into_iter().collect(),
        script: Box::new(|wire| {
            wire.event(closed(WINDOW))?;
            wire.finish()
        }),
        log,
    };
    run_with_backend::<StandIn>(app, scenario, |backend| {
        backend.with_scene(Box::new(make_scene))
    })?;
    assert_eq!(scenes.try_iter().count(), 1);
    Ok(())
}

#[test]
fn without_a_renderer_frames_go_on_at_the_refresh_of_the_output_and_none_is_drawn() -> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    let frames = Rc::new(Cell::new(0_u32));
    let counted = Rc::clone(&frames);
    setup.observers.frame = Some(Box::new(move |_: FrameTime| counted.set(counted.get() + 1)));
    let drawn = Rc::new(Cell::new(false));
    let submitted = Rc::clone(&drawn);
    setup.observers.frame_report = Some(Box::new(move |_: FrameReport| submitted.set(true)));
    let viewed = Rc::clone(&drawn);
    setup.observers.view = Some(Box::new(move |_, _, _, _: &View| viewed.set(true)));
    setup.run_end.exit_when(move || frames.get() == 3);
    let run = run_scripted_on(None, host([OUTPUT]), Capability::ALL, setup, |wire| {
        wire.attach(WINDOW)?;
        wire.event(PlatformEvent::Output(OutputEvent::Added(OUTPUT)))?;
        wire.event(PlatformEvent::Output(OutputEvent::Entered(OUTPUT, WINDOW)))?;
        let refresh = Some(Duration::from_millis(1));
        wire.event(PlatformEvent::Output(OutputEvent::Refresh(OUTPUT, refresh)))?;
        wire.event(configured())?;
        wire.finish()
    });
    run.outcome?;
    assert!(!drawn.get());
    Ok(())
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
