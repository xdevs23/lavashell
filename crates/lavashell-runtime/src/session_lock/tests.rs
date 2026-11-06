// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use lavashell_core::geometry::{Size, Viewport};
use lavashell_core::units::{Density, DpExt};
use lavashell_test_fixtures::{NoText, RecordedScene, WithoutFailures};

const FIRST: OutputId = OutputId(1);
const SECOND: OutputId = OutputId(2);

fn configuration(handle: &SessionLock) -> LockConfiguration {
    LockConfiguration::new(handle.clone(), lavashell_test_fixtures::facts(), |_| {})
}

/// Opens `surface` on `output` with the view offered by `lock`.
fn open(lock: &mut ActiveLock, output: OutputId, surface: SurfaceId) -> TestResult {
    let _view = lock.view_for(output).ok_or(TestError::NoLockView)?;
    lock.opened(output, surface);
    Ok(())
}

/// A lock started on both outputs, with surface 1 on the first output and surface 2 on the second.
fn locked_on_both(handle: &SessionLock) -> Result<ActiveLock, TestError> {
    let mut lock = ActiveLock::start(configuration(handle), &[FIRST, SECOND])?;
    open(&mut lock, FIRST, SurfaceId(1))?;
    open(&mut lock, SECOND, SurfaceId(2))?;
    Ok(lock)
}

#[test]
fn release_is_scoped_to_one_attached_lifecycle() -> TestResult {
    let handle = SessionLock::new();
    handle.unlock();
    assert!(!handle.release_requested());
    handle.attach()?;
    handle.clone().unlock();
    assert!(handle.release_requested());
    handle.release();
    assert!(!handle.release_requested());
    assert!(handle.attach().is_err());
    let next = SessionLock::new();
    next.attach()?;
    next.unlock();
    assert!(next.release_requested());
    Ok(())
}

#[test]
fn a_lock_needs_an_output_and_a_fresh_handle() {
    let handle = SessionLock::new();
    let without_output = ActiveLock::start(configuration(&handle), &[]);
    assert!(matches!(
        without_output,
        Err(Error::SessionLockWithoutOutput)
    ));
    assert!(ActiveLock::start(configuration(&handle), &[FIRST]).is_ok());
    let reused = ActiveLock::start(configuration(&handle), &[FIRST]);
    assert!(matches!(reused, Err(Error::SessionLockReused)));
}

#[test]
fn each_output_gets_one_lock_surface_until_it_is_removed() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = locked_on_both(&handle)?;
    assert!(lock.view_for(FIRST).is_none());
    assert_eq!(lock.remove_output(FIRST), Some(SurfaceId(1)));
    assert_eq!(lock.remove_output(FIRST), None);
    open(&mut lock, FIRST, SurfaceId(3))?;
    assert_eq!(lock.remove_output(FIRST), Some(SurfaceId(3)));
    Ok(())
}

#[test]
fn an_output_is_offered_a_view_until_its_lock_surface_opened() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = ActiveLock::start(configuration(&handle), &[FIRST])?;
    assert!(lock.view_for(FIRST).is_some());
    assert!(lock.view_for(FIRST).is_some());
    assert_eq!(lock.remove_output(FIRST), None);
    lock.opened(FIRST, SurfaceId(1));
    assert!(lock.view_for(FIRST).is_none());
    Ok(())
}

#[test]
fn an_unlock_waits_for_the_lock_and_closes_every_lock_surface() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = locked_on_both(&handle)?;
    handle.unlock();
    assert_eq!(lock.step()?, LockStep::Running);
    lock.handle(LockEvent::Locked);
    assert_eq!(lock.step()?, LockStep::Unlock);
    assert_eq!(lock.step()?, LockStep::Running);
    assert_eq!(lock.remove_output(FIRST), None);
    assert_eq!(lock.remove_output(SECOND), None);
    Ok(())
}

#[test]
fn outputs_that_appear_during_the_unlock_get_no_lock_surface() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = locked_on_both(&handle)?;
    lock.handle(LockEvent::Locked);
    handle.unlock();
    assert_eq!(lock.step()?, LockStep::Unlock);
    assert!(lock.view_for(OutputId(3)).is_none());
    lock.handle(LockEvent::UnlockConfirmed);
    assert!(lock.view_for(OutputId(3)).is_none());
    Ok(())
}

#[test]
fn the_confirmed_unlock_releases_the_handle() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = locked_on_both(&handle)?;
    lock.handle(LockEvent::UnlockConfirmed);
    assert_eq!(lock.step()?, LockStep::Running);
    lock.handle(LockEvent::Locked);
    handle.unlock();
    assert_eq!(lock.step()?, LockStep::Unlock);
    assert!(handle.release_requested());
    lock.handle(LockEvent::UnlockConfirmed);
    assert_eq!(lock.step()?, LockStep::Released);
    assert!(!handle.release_requested());
    Ok(())
}

#[test]
fn a_lock_ended_by_the_host_opens_no_surface_and_fails_the_next_step() -> TestResult {
    let handle = SessionLock::new();
    let mut lock = locked_on_both(&handle)?;
    lock.handle(LockEvent::Locked);
    lock.handle(LockEvent::Finished);
    lock.handle(LockEvent::Locked);
    assert!(lock.view_for(OutputId(3)).is_none());
    assert!(matches!(lock.step(), Err(Error::SessionLockEnded)));
    Ok(())
}

#[test]
fn every_lock_surface_shows_a_view_of_the_same_compose_function() -> TestResult {
    let composed = Rc::new(Cell::new(0));
    let counted = composed.clone();
    let configuration = LockConfiguration::new(
        SessionLock::new(),
        lavashell_test_fixtures::facts(),
        move |_| counted.set(counted.get() + 1),
    );
    let mut lock = ActiveLock::start(configuration, &[FIRST, SECOND])?;
    let mut views = Vec::new();
    for (output, id) in [(FIRST, 1), (SECOND, 2)] {
        views.push(lock.view_for(output).ok_or(TestError::NoLockView)?);
        lock.opened(output, SurfaceId(id));
    }
    let viewport = Viewport::new(Size::new(100.dp(), 50.dp()), Density::ONE);
    for view in &mut views {
        (view.frame_into(viewport, &mut NoText, &mut RecordedScene::default()))
            .without_failures()?;
    }
    assert_eq!(composed.get(), 2);
    Ok(())
}
