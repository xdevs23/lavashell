// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use crate::test_host::empty_view;
use lavashell_platform::{WatchEvent, WatchRequest};
use std::cell::Cell;

fn readable_source() -> Result<OwnedFd, TestError> {
    let (reader, _writer) = std::io::pipe()?;
    Ok(OwnedFd::from(reader))
}

fn watched(requests: &[WatchRequest]) -> Vec<WatchId> {
    (requests.iter())
        .filter_map(|request| match request {
            WatchRequest::Watch(id, ..) => Some(*id),
            WatchRequest::Resume(_) | WatchRequest::Unwatch(_) => None,
        })
        .collect()
}

fn unwatched(requests: &[WatchRequest]) -> Vec<WatchId> {
    (requests.iter())
        .filter_map(|request| match request {
            WatchRequest::Unwatch(id) => Some(*id),
            WatchRequest::Watch(..) | WatchRequest::Resume(_) => None,
        })
        .collect()
}

#[test]
fn actions_queued_while_a_batch_runs_wait_for_the_next_batch() {
    let queue = ActionQueue::new(&Watches::default());
    queue.open_surface(SurfaceId::numbered(3));
    queue.open_surface(SurfaceId::numbered(4));
    let actions = queue.view_actions();
    let again = actions.clone();
    actions.dispatch(SurfaceId::numbered(3), move |_| {
        again.dispatch(SurfaceId::numbered(4), |_| {});
    });
    let mut batch = queue.take_batch();
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
    batch.run(SurfaceId::numbered(3), &mut empty_view());
    assert!(queue.has_pending_for(SurfaceId::numbered(4)));
    queue.requeue(batch);
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
}

#[test]
fn actions_that_did_not_run_go_before_actions_queued_during_the_batch() {
    let queue = ActionQueue::new(&Watches::default());
    queue.open_surface(SurfaceId::numbered(3));
    let actions = queue.view_actions();
    let order = Rc::new(RefCell::new(Vec::new()));
    let record = |tag: char| {
        let order = order.clone();
        move |_: &mut View| order.borrow_mut().push(tag)
    };
    actions.dispatch(SurfaceId::numbered(3), record('A'));
    actions.dispatch(SurfaceId::numbered(3), record('B'));
    let batch = queue.take_batch();
    actions.dispatch(SurfaceId::numbered(3), record('C'));
    queue.requeue(batch);
    queue
        .take_batch()
        .run(SurfaceId::numbered(3), &mut empty_view());
    assert_eq!(*order.borrow(), ['A', 'B', 'C']);
}

#[test]
fn dispatch_after_close_is_dropped() -> TestResult {
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
    queue.open_surface(SurfaceId::numbered(3));
    let actions = queue.view_actions();
    let held = Rc::new(());
    let captured = held.clone();
    actions.dispatch_when_readable(readable_source()?, SurfaceId::numbered(3), move |_| {
        drop(captured);
    })?;
    let ids = watched(&watches.take_requests());
    queue.close();
    assert_eq!(Rc::strong_count(&held), 1);
    actions.dispatch(SurfaceId::numbered(3), |_| {});
    actions.dispatch_when_readable(readable_source()?, SurfaceId::numbered(3), |_| {})?;
    watches.handle(WatchEvent::Ready(ids[0]));
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
    assert!(watched(&watches.take_requests()).is_empty());
    Ok(())
}

#[test]
fn queues_a_watched_action_once_its_descriptor_is_readable() -> TestResult {
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
    queue.open_surface(SurfaceId::numbered(3));
    let ran = Rc::new(Cell::new(0));
    let counter = ran.clone();
    queue.view_actions().dispatch_when_readable(
        readable_source()?,
        SurfaceId::numbered(3),
        move |_| {
            counter.set(counter.get() + 1);
        },
    )?;
    let ids = watched(&watches.take_requests());
    assert_eq!(ids.len(), 1);
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
    for id in ids.iter().chain(&ids) {
        watches.handle(WatchEvent::Ready(*id));
    }
    let mut batch = queue.take_batch();
    batch.run(SurfaceId::numbered(3), &mut empty_view());
    queue.requeue(batch);
    assert_eq!(ran.get(), 1);
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
    assert_eq!(unwatched(&watches.take_requests()), ids);
    Ok(())
}

#[test]
fn actions_for_a_surface_that_never_opened_are_dropped() {
    let queue = ActionQueue::new(&Watches::default());
    queue
        .view_actions()
        .dispatch(SurfaceId::numbered(3), |_| {});
    assert!(!queue.has_pending_for(SurfaceId::numbered(3)));
}

#[test]
fn forgetting_a_surface_drops_the_actions_and_watches_for_it() -> TestResult {
    let (forgotten, open) = (SurfaceId::numbered(3), SurfaceId::numbered(4));
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
    queue.open_surface(forgotten);
    queue.open_surface(open);
    let actions = queue.view_actions();
    actions.dispatch(forgotten, |_| {});
    actions.dispatch(open, |_| {});
    actions.dispatch_when_readable(readable_source()?, forgotten, |_| {})?;
    actions.dispatch_when_readable(readable_source()?, open, |_| {})?;
    let ids = watched(&watches.take_requests());
    let batch = queue.take_batch();
    actions.dispatch(forgotten, |_| {});
    queue.forget_surface(forgotten);
    queue.requeue(batch);
    actions.dispatch(forgotten, |_| {});
    assert_eq!(unwatched(&watches.take_requests()), ids[..1]);
    assert!(!queue.has_pending_for(forgotten));
    assert!(queue.has_pending_for(open));
    let _taken_batch_is_not_checked = queue.take_batch();
    for id in &ids {
        watches.handle(WatchEvent::Ready(*id));
    }
    assert!(!queue.has_pending_for(forgotten));
    assert!(queue.has_pending_for(open));
    Ok(())
}
