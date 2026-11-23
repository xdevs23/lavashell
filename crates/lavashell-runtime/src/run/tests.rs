// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::controls_actor::ControlsError;
use crate::panels::KeyboardPolicy;
use crate::renderer_actor::RendererError;
use crate::test_error::TestResult;
use crate::test_host::{
    Scenario, StandIn, StandInError, closed, empty_view, end_on_marker, host, run_scripted,
    run_scripted_on, run_unreachable, runtime_setup, surface_requests, window,
};
use crate::test_renderer::TestBackend;
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime, PresentationFlags};
use lavashell_core::geometry::{Size, Viewport};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::DpExt;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};
use lavashell_platform::{
    FocusGrabRequest, OutputEvent, OutputId, PlatformEvent, PlatformRequest, SurfaceEvent,
    SurfaceRequest, TimingEvent,
};
use std::cell::{Cell, RefCell};
use std::panic::resume_unwind;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

const WINDOW: SurfaceId = SurfaceId::numbered(0);
const PANEL: SurfaceId = SurfaceId::numbered(0);
const OUTPUT: OutputId = OutputId(1);

fn configured(surface: SurfaceId, size: Size) -> PlatformEvent {
    PlatformEvent::Surface(
        surface,
        SurfaceEvent::Configured {
            width: Some(size.width),
            height: Some(size.height),
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
    release_after_detach(Some(TestBackend::default()))
}

#[test]
fn a_destroyed_surface_is_released_once_the_platform_detached_it_without_a_renderer() -> TestResult
{
    release_after_detach(None)
}

fn release_after_detach(backend: Option<TestBackend>) -> TestResult {
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
        wire.event(configured(WINDOW, Size::new(100.dp(), 100.dp())))?;
        wire.finish()
    });
    assert!(matches!(
        run.outcome,
        Err(RunError::RunRenderer(JoinError::Failed(
            RendererError::Draw(ControlsError::UnknownSurface(WINDOW))
        )))
    ));
}

#[test]
fn the_backend_that_the_app_configures_is_the_one_the_renderer_starts() -> TestResult {
    let mut app = App::new().facts(lavashell_test_fixtures::facts());
    app.window(Window::new("window", "test.app"), |_| {})?;
    let (start_notice, starts) = mpsc::channel();
    let (log, _requests_kept_for_the_log) = mpsc::channel();
    let scenario = Scenario::Scripted {
        backend: Some(TestBackend::default()),
        host: host([]),
        offered: Capability::ALL.into_iter().collect(),
        script: Box::new(|wire| {
            wire.event(closed(WINDOW))?;
            wire.finish()
        }),
        log,
    };
    run_with_backend::<StandIn>(app, scenario, |_unconfigured| {
        TestBackend::noticing_start(start_notice)
    })?;
    assert_eq!(starts.try_iter().count(), 1);
    Ok(())
}

#[test]
fn without_a_renderer_frames_go_on_at_the_refresh_of_the_output_none_is_drawn_and_none_is_viewed_twice()
-> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    let frames = Rc::new(Cell::new(0_u32));
    let counted = Rc::clone(&frames);
    setup.observers.frame = Some(Box::new(move |_: FrameTime| counted.set(counted.get() + 1)));
    let drawn = Rc::new(Cell::new(false));
    let submitted = Rc::clone(&drawn);
    setup.observers.frame_report = Some(Box::new(move |_: FrameReport| submitted.set(true)));
    let viewed = Rc::new(RefCell::new(Vec::new()));
    let views = Rc::clone(&viewed);
    setup.observers.view = Some(Box::new(move |_, timing: FrameTime, _, _: &View| {
        views.borrow_mut().push(timing.sequence);
    }));
    setup.run_end.exit_when(move || frames.get() == 3);
    let run = run_scripted_on(None, host([OUTPUT]), Capability::ALL, setup, |wire| {
        wire.attach(WINDOW)?;
        wire.event(PlatformEvent::Output(OutputEvent::Added(OUTPUT)))?;
        wire.event(PlatformEvent::Output(OutputEvent::Entered(OUTPUT, WINDOW)))?;
        let refresh = Some(Duration::from_millis(1));
        wire.event(PlatformEvent::Output(OutputEvent::Refresh(OUTPUT, refresh)))?;
        wire.event(configured(WINDOW, Size::new(100.dp(), 100.dp())))?;
        wire.finish()
    });
    run.outcome?;
    assert!(!drawn.get());
    let viewed = viewed.take();
    assert_ne!(viewed, Vec::<u64>::new());
    assert_eq!(BTreeSet::from_iter(&viewed).len(), viewed.len());
    Ok(())
}

#[test]
fn without_a_renderer_the_runtime_views_a_committed_panel_at_its_size_and_acquires_the_focus_grab()
-> TestResult {
    let mut launcher = LayerSurface::new("launcher").focus_grab(FocusGrab::new(|| true, || {}));
    launcher.panel.keyboard = KeyboardPolicy::OnDemand;
    let mut setup = runtime_setup(vec![SurfaceSetup::Panel(launcher, empty_view())]);
    let viewed = Rc::new(Cell::new(None));
    let seen = Rc::clone(&viewed);
    setup.observers.view = Some(Box::new(move |_, _, viewport: Viewport, _: &View| {
        seen.set(Some(viewport.size));
    }));
    let shown = Rc::clone(&viewed);
    setup.run_end.exit_when(move || shown.get().is_some());
    let size = Size::new(1366.dp(), 48.dp());
    let run = run_scripted_on(None, host([]), Capability::ALL, setup, move |wire| {
        wire.attach(PANEL)?;
        wire.event(configured(PANEL, size))?;
        wire.finish()
    });
    let requests = run.requests()?;
    assert_eq!(viewed.get(), Some(size));
    assert!(requests.iter().any(|request| matches!(
        request,
        PlatformRequest::FocusGrab(FocusGrabRequest::Acquire(PANEL))
    )));
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

impl NamesNextStep for StandInError {
    fn next_step(&self) -> NextStep {
        NextStep::FreeGpuMemory
    }
}

#[test]
fn a_run_error_passes_up_the_next_step_of_the_platform_or_the_renderer_error_it_wraps() {
    let platform = StandInError::Failed;
    let step = platform.next_step();
    assert_eq!(RunError::<StandIn>::Platform(platform).next_step(), step);
    let shaper = lavashell_renderer::Error::SubpixelTileSize;
    let step = shaper.next_step();
    assert_eq!(RunError::<StandIn>::CreateShaper(shaper).next_step(), step);
}

#[test]
fn a_renderer_failure_takes_the_next_step_of_the_renderer_error_it_wraps() {
    let painter = lavashell_renderer::Error::SubpixelTileSize;
    let step = painter.next_step();
    let failed =
        RunError::<StandIn>::StartRenderer(SpawnError::Start(RendererError::Start(painter)));
    assert_eq!(failed.next_step(), step);
}

#[test]
fn a_renderer_that_fails_while_running_takes_the_next_step_of_the_error_it_wraps() {
    let unknown = ControlsError::UnknownSurface(WINDOW);
    let step = unknown.next_step();
    let failed = RunError::<StandIn>::RunRenderer(JoinError::Failed(RendererError::Draw(unknown)));
    assert_eq!(failed.next_step(), step);
}

#[test]
fn a_scene_pipeline_without_a_thread_asks_to_free_system_resources() {
    let failed =
        RunError::<StandIn>::SceneFailed(Arc::new(scene_pipeline::Error::StartChoreographer(
            SpawnError::Thread(std::io::Error::from_raw_os_error(11)),
        )));
    assert_eq!(failed.next_step(), NextStep::FreeSystemResources);
}
