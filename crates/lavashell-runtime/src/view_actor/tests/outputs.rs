// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::layer_surface::LayerSurface;
use crate::panels::PanelsPerOutput;
use crate::run::RunError;
use crate::session_lock::{self, LockConfiguration, SessionLock};
use lavashell_core::output::MonitorInfo;
use lavashell_platform::{OutputEvent, OutputId, SessionLockEvent};

const SURFACE_0: SurfaceId = SurfaceId::numbered(0);
const SURFACE_1: SurfaceId = SurfaceId::numbered(1);

const FIRST: OutputId = OutputId(1);
const SECOND: OutputId = OutputId(2);
const THIRD: OutputId = OutputId(3);

fn monitor(width: i32) -> MonitorInfo {
    MonitorInfo {
        name: Some("DP-1".to_owned()),
        size: Size::new(width.dp(), 1080.dp()),
        physical_size: None,
    }
}

fn output(event: OutputEvent) -> PlatformEvent {
    PlatformEvent::Output(event)
}

fn lock(event: SessionLockEvent) -> PlatformEvent {
    PlatformEvent::Lock(event)
}

fn bars_on_every_output() -> RuntimeSetup {
    let mut setup = runtime_setup(Vec::new());
    setup.panels_per_output = Some(PanelsPerOutput::new(|_| {
        vec![(LayerSurface::new("bar"), empty_view())]
    }));
    setup
}

fn locking(handle: &SessionLock) -> RuntimeSetup {
    let mut setup = runtime_setup(Vec::new());
    let platform = lavashell_test_fixtures::facts();
    setup.session_lock = Some(LockConfiguration::new(handle.clone(), platform, |_| {}));
    setup
}

fn panel_outputs(requests: &[PlatformRequest]) -> Vec<(SurfaceId, Option<OutputId>)> {
    (created(requests).into_iter())
        .filter_map(|(surface, spec)| match spec.role {
            Role::Panel(panel) => Some((surface, panel.output)),
            Role::Window(_) | Role::Lock { .. } => None,
        })
        .collect()
}

fn lock_outputs(requests: &[PlatformRequest]) -> Vec<(SurfaceId, OutputId)> {
    (created(requests).into_iter())
        .filter_map(|(surface, spec)| match spec.role {
            Role::Lock { output } => Some((surface, output)),
            Role::Window(_) | Role::Panel(_) => None,
        })
        .collect()
}

#[test]
fn an_output_gets_its_panels_once_the_host_reported_its_geometry() -> TestResult {
    let mut views = Views::start(bars_on_every_output(), Capability::ALL, host([]))?;
    let added = views.step([output(OutputEvent::Added(FIRST))])?;
    assert_eq!(panel_outputs(&added), Vec::new());
    let updated = views.step([output(OutputEvent::Updated(FIRST, monitor(1920)))])?;
    assert_eq!(
        panel_outputs(&updated),
        [(SurfaceId::numbered(0), Some(FIRST))]
    );
    let resized = views.step([output(OutputEvent::Updated(FIRST, monitor(2560)))])?;
    assert_eq!(panel_outputs(&resized), Vec::new());
    Ok(())
}

#[test]
fn the_panels_of_an_output_close_when_the_host_removes_the_output() -> TestResult {
    let mut views = Views::start(bars_on_every_output(), Capability::ALL, host([]))?;
    let opened = views.step([output(OutputEvent::Updated(FIRST, monitor(1920)))])?;
    assert_eq!(
        panel_outputs(&opened),
        [(SurfaceId::numbered(0), Some(FIRST))]
    );
    let removed = views.step([output(OutputEvent::Removed(FIRST))])?;
    assert!(matches!(
        surface_requests(&removed).as_slice(),
        [SurfaceRequest::Destroy(SURFACE_0)]
    ));
    Ok(())
}

#[test]
fn panels_per_output_keep_a_run_without_surfaces_going() -> TestResult {
    let mut without = Views::start(runtime_setup(Vec::new()), Capability::ALL, host([]))?;
    assert!(without.actor.finished());
    let mut with = Views::start(bars_on_every_output(), Capability::ALL, host([]))?;
    assert!(!with.actor.finished());
    Ok(())
}

#[test]
fn a_session_lock_is_requested_before_its_surfaces_one_per_output() -> TestResult {
    let mut views = Views::start(
        locking(&SessionLock::new()),
        Capability::ALL,
        host([FIRST, SECOND]),
    )?;
    let requests = views.step([])?;
    assert!(matches!(
        requests.first(),
        Some(PlatformRequest::Lock(SessionLockRequest::Lock))
    ));
    assert_eq!(
        lock_outputs(&requests),
        [
            (SurfaceId::numbered(0), FIRST),
            (SurfaceId::numbered(1), SECOND)
        ]
    );
    assert!(!views.actor.finished());
    Ok(())
}

#[test]
fn an_output_that_appears_while_the_session_is_locked_gets_a_lock_surface() -> TestResult {
    let mut views = Views::start(locking(&SessionLock::new()), Capability::ALL, host([FIRST]))?;
    let started = views.step([])?;
    assert_eq!(lock_outputs(&started), [(SurfaceId::numbered(0), FIRST)]);
    let known = views.step([output(OutputEvent::Added(FIRST))])?;
    assert_eq!(lock_outputs(&known), Vec::new());
    let added = views.step([output(OutputEvent::Added(THIRD))])?;
    assert_eq!(lock_outputs(&added), [(SurfaceId::numbered(1), THIRD)]);
    Ok(())
}

#[test]
fn the_lock_surface_of_an_output_closes_when_the_host_removes_the_output() -> TestResult {
    let mut views = Views::start(
        locking(&SessionLock::new()),
        Capability::ALL,
        host([FIRST, SECOND]),
    )?;
    let started = views.step([])?;
    assert_eq!(lock_outputs(&started).len(), 2);
    let removed = views.step([output(OutputEvent::Removed(SECOND))])?;
    assert!(matches!(
        surface_requests(&removed).as_slice(),
        [SurfaceRequest::Destroy(SURFACE_1)]
    ));
    Ok(())
}

#[test]
fn an_unlock_destroys_the_lock_surfaces_and_its_confirmation_ends_the_run() -> TestResult {
    let handle = SessionLock::new();
    let mut views = Views::start(locking(&handle), Capability::ALL, host([FIRST, SECOND]))?;
    let locked = views.step([lock(SessionLockEvent::Locked)])?;
    assert_eq!(lock_outputs(&locked).len(), 2);
    handle.unlock();
    let unlocking = views.step([])?;
    assert!(matches!(
        unlocking.first(),
        Some(PlatformRequest::Lock(SessionLockRequest::Unlock))
    ));
    assert!(matches!(
        surface_requests(&unlocking).as_slice(),
        [
            SurfaceRequest::Destroy(SURFACE_0),
            SurfaceRequest::Destroy(SURFACE_1)
        ]
    ));
    assert!(!views.actor.finished());
    let confirmed = views.step([lock(SessionLockEvent::UnlockConfirmed)])?;
    assert!(confirmed.is_empty());
    assert!(views.actor.finished());
    Ok(())
}

#[test]
fn a_host_that_ends_the_lock_ends_the_views_with_an_error() -> TestResult {
    let mut views = Views::start(locking(&SessionLock::new()), Capability::ALL, host([FIRST]))?;
    let ended = views.step([lock(SessionLockEvent::Finished)]);
    assert!(matches!(
        ended,
        Err(TestError::View(ViewError::SessionLock(
            session_lock::Error::Ended
        )))
    ));
    Ok(())
}

#[test]
fn a_session_lock_cannot_start_on_a_host_without_outputs() {
    let started = Views::start(locking(&SessionLock::new()), Capability::ALL, host([]));
    assert!(matches!(
        started,
        Err(TestError::Run(RunError::View(ViewError::SessionLock(
            session_lock::Error::WithoutOutput
        ))))
    ));
}
