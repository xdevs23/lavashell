// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestError;
use lavashell_actor::{Receiver, mailbox};
use lavashell_platform::PlatformEvent;
use sctk::reexports::calloop::EventLoop;
use std::time::Duration;

const WATCH: WatchId = WatchId(7);

/// Runs the sources of `event_loop` that are ready, without waiting, and returns the watches reported to `view`.
fn reported_after_turn(
    event_loop: &mut EventLoop<'static, ()>,
    view: &Receiver<PlatformEvent>,
) -> Result<Vec<WatchId>, TestError> {
    event_loop.dispatch(Duration::ZERO, &mut ())?;
    let reported = view
        .take_arrived()?
        .into_iter()
        .filter_map(|event| match event {
            PlatformEvent::Watch(WatchEvent::Ready(id)) => Some(id),
            _ => None,
        });
    Ok(reported.collect())
}

#[test]
fn a_ready_watch_is_reported_once_until_the_runtime_resumes_it() -> Result<(), TestError> {
    let mut event_loop: EventLoop<'static, ()> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let (sender, receiver) = mailbox::<PlatformEvent>();
    let view = ViewLink::new(sender.outlet(|event| event));
    let (_reader, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
    let mut watches = WatchSources::default();
    let watch = WatchRequest::Watch(WATCH, writer, Readiness::Writable);
    watches.execute(&handle, watch, &view)?;
    assert_eq!(reported_after_turn(&mut event_loop, &receiver)?, [WATCH]);
    assert!(reported_after_turn(&mut event_loop, &receiver)?.is_empty());
    watches.execute(&handle, WatchRequest::Resume(WATCH), &view)?;
    assert_eq!(reported_after_turn(&mut event_loop, &receiver)?, [WATCH]);
    Ok(())
}

#[test]
fn an_ended_watch_is_not_reported_after_a_resume() -> Result<(), TestError> {
    let mut event_loop: EventLoop<'static, ()> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let (sender, receiver) = mailbox::<PlatformEvent>();
    let view = ViewLink::new(sender.outlet(|event| event));
    let (_reader, writer) = rustix::pipe::pipe().map_err(TestError::CreatePipe)?;
    let mut watches = WatchSources::default();
    let watch = WatchRequest::Watch(WATCH, writer, Readiness::Writable);
    watches.execute(&handle, watch, &view)?;
    assert_eq!(reported_after_turn(&mut event_loop, &receiver)?, [WATCH]);
    watches.execute(&handle, WatchRequest::Unwatch(WATCH), &view)?;
    watches.execute(&handle, WatchRequest::Resume(WATCH), &view)?;
    assert!(reported_after_turn(&mut event_loop, &receiver)?.is_empty());
    Ok(())
}
