// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_platform::{WatchEvent, WatchRequest};
use std::cell::Cell;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("the runtime failed")]
    Runtime(#[from] Error),
    #[error("a pipe could not be created")]
    CreatePipe(#[from] std::io::Error),
}

type TestResult = Result<(), TestError>;

fn empty_view() -> View {
    View::new(lavashell_test_fixtures::facts(), |_| {})
}

fn readable_source() -> Result<OwnedFd, TestError> {
    let (reader, _writer) = std::io::pipe()?;
    Ok(OwnedFd::from(reader))
}

fn watched(requests: &[WatchRequest]) -> Vec<WatchId> {
    (requests.iter())
        .filter_map(|request| match request {
            WatchRequest::Watch(id, ..) => Some(*id),
            WatchRequest::Unwatch(_) => None,
        })
        .collect()
}

fn unwatched(requests: &[WatchRequest]) -> Vec<WatchId> {
    (requests.iter())
        .filter_map(|request| match request {
            WatchRequest::Unwatch(id) => Some(*id),
            WatchRequest::Watch(..) => None,
        })
        .collect()
}

#[test]
fn actions_queued_while_a_batch_runs_wait_for_the_next_batch() {
    let queue = ActionQueue::new(&Watches::default());
    let actions = queue.view_actions();
    let again = actions.clone();
    actions.dispatch(SurfaceId(3), move |_| again.dispatch(SurfaceId(4), |_| {}));
    let mut batch = queue.take_batch();
    assert!(!queue.has_pending());
    assert!(batch.has_actions_for(SurfaceId(3)));
    batch.run(SurfaceId(3), &mut empty_view());
    assert!(!batch.has_actions_for(SurfaceId(3)));
    assert!(queue.take_batch().has_actions_for(SurfaceId(4)));
}

#[test]
fn actions_that_did_not_run_go_before_actions_queued_during_the_batch() {
    let queue = ActionQueue::new(&Watches::default());
    let actions = queue.view_actions();
    let order = Rc::new(RefCell::new(Vec::new()));
    let record = |tag: char| {
        let order = order.clone();
        move |_: &mut View| order.borrow_mut().push(tag)
    };
    actions.dispatch(SurfaceId(3), record('A'));
    actions.dispatch(SurfaceId(3), record('B'));
    let batch = queue.take_batch();
    actions.dispatch(SurfaceId(3), record('C'));
    queue.requeue(batch);
    queue.take_batch().run(SurfaceId(3), &mut empty_view());
    assert_eq!(*order.borrow(), ['A', 'B', 'C']);
}

#[test]
fn dispatch_after_close_is_dropped() -> TestResult {
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
    let actions = queue.view_actions();
    let held = Rc::new(());
    let captured = held.clone();
    actions.dispatch_when_readable(readable_source()?, SurfaceId(3), move |_| drop(captured))?;
    let ids = watched(&watches.take_requests());
    queue.close();
    assert_eq!(Rc::strong_count(&held), 1);
    actions.dispatch(SurfaceId(3), |_| {});
    actions.dispatch_when_readable(readable_source()?, SurfaceId(3), |_| {})?;
    watches.handle(WatchEvent::Ready(ids[0]));
    assert!(!queue.has_pending());
    assert!(watched(&watches.take_requests()).is_empty());
    Ok(())
}

#[test]
fn queues_a_watched_action_once_its_descriptor_is_readable() -> TestResult {
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
    let ran = Rc::new(Cell::new(0));
    let counter = ran.clone();
    queue
        .view_actions()
        .dispatch_when_readable(readable_source()?, SurfaceId(3), move |_| {
            counter.set(counter.get() + 1);
        })?;
    let ids = watched(&watches.take_requests());
    assert_eq!(ids.len(), 1);
    assert!(!queue.has_pending());
    for id in ids.iter().chain(&ids) {
        watches.handle(WatchEvent::Ready(*id));
    }
    let mut batch = queue.take_batch();
    batch.run(SurfaceId(3), &mut empty_view());
    queue.requeue(batch);
    assert_eq!(ran.get(), 1);
    assert!(!queue.has_pending());
    assert_eq!(unwatched(&watches.take_requests()), ids);
    Ok(())
}

#[test]
fn forgetting_a_surface_drops_the_actions_and_watches_for_it() -> TestResult {
    let (forgotten, open) = (SurfaceId(3), SurfaceId(4));
    let watches = Watches::default();
    let queue = ActionQueue::new(&watches);
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
    let batch = queue.take_batch();
    assert!(!batch.has_actions_for(forgotten));
    assert!(batch.has_actions_for(open));
    for id in &ids {
        watches.handle(WatchEvent::Ready(*id));
    }
    let batch = queue.take_batch();
    assert!(!batch.has_actions_for(forgotten));
    assert!(batch.has_actions_for(open));
    Ok(())
}
