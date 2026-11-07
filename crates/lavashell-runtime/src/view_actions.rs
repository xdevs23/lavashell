// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{WatchEvent, WatchId, WatchRequest};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::os::fd::OwnedFd;
use std::rc::Rc;

struct PendingAction {
    surface: SurfaceId,
    apply: Box<dyn FnOnce(&mut View)>,
}

impl PendingAction {
    fn new(surface: SurfaceId, apply: impl FnOnce(&mut View) + 'static) -> Self {
        Self {
            surface,
            apply: Box::new(apply),
        }
    }
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<PendingAction>,
    /// The actions to queue once the descriptor watched under the same id is readable.
    watches: BTreeMap<WatchId, PendingAction>,
    requests: Vec<WatchRequest>,
    /// The number of watches requested so far, which is the id of the next watch.
    watch_count: u64,
    forgotten: BTreeSet<SurfaceId>,
    closed: bool,
}

impl Queue {
    fn accepts(&self, surface: SurfaceId) -> bool {
        !self.closed && !self.forgotten.contains(&surface)
    }
}

/// The handle through which controls and asynchronous transfer callbacks add actions to an `ActionQueue`. An action
/// that requests a transfer captures the text it needs when it is queued, because the focus may have moved by the time
/// the data arrives.
#[derive(Clone)]
pub struct ViewActions(Rc<RefCell<Queue>>);

impl ViewActions {
    /// Queues `apply` for the view of `surface`. The action is dropped when that surface closes or is retired
    /// before the action runs.
    pub fn dispatch(&self, surface: SurfaceId, apply: impl FnOnce(&mut View) + 'static) {
        let mut queue = self.0.borrow_mut();
        if queue.accepts(surface) {
            queue.pending.push_back(PendingAction::new(surface, apply));
        }
    }

    /// Queues `apply` once `source` becomes readable, such as the pidfd of a child process when it exits. `source` is
    /// closed after that. The action and `source` are dropped when that surface closes before `source` is readable.
    ///
    /// # Errors
    ///
    /// Fails when the number of the new watch does not fit a watch id.
    pub fn dispatch_when_readable(
        &self,
        source: OwnedFd,
        surface: SurfaceId,
        apply: impl FnOnce(&mut View) + 'static,
    ) -> Result<(), Error> {
        let mut queue = self.0.borrow_mut();
        if !queue.accepts(surface) {
            return Ok(());
        }
        let id = WatchId(queue.watch_count);
        queue.watch_count = id.0.checked_add(1).ok_or(Error::NumberWatch)?;
        queue.requests.push(WatchRequest::Readable(id, source));
        let action = PendingAction::new(surface, apply);
        let _no_watch_had_the_id = queue.watches.insert(id, action);
        Ok(())
    }
}

/// A queue of actions. Each action changes the view of one surface. Controls and asynchronous transfer callbacks add
/// actions to it through `ViewActions`. The runtime runs each action after it composes the view of that surface.
#[derive(Default)]
pub struct ActionQueue(Rc<RefCell<Queue>>);

impl ActionQueue {
    /// A handle that adds actions to this queue.
    #[must_use]
    pub fn view_actions(&self) -> ViewActions {
        ViewActions(self.0.clone())
    }

    /// Removes the queued actions as a batch. The actions queued while the batch runs wait for the next batch.
    pub fn take_batch(&self) -> ActionBatch {
        ActionBatch(self.0.borrow_mut().pending.drain(..).collect())
    }

    /// Puts the actions of `batch` that did not run back at the front of the queue, in the order in which they were
    /// queued. An action for a forgotten surface is dropped.
    pub fn requeue(&self, batch: ActionBatch) {
        let mut queue = self.0.borrow_mut();
        let (kept, dropped): (Vec<_>, Vec<_>) = batch
            .0
            .into_iter()
            .partition(|action| queue.accepts(action.surface));
        for action in kept.into_iter().rev() {
            queue.pending.push_front(action);
        }
        drop(queue);
        drop(dropped);
    }

    /// Drops the actions and watches for `surface`, which retired, and every action queued for that surface later.
    /// Requests the end of each dropped watch, so the platform closes the watched descriptor at once.
    pub fn forget_surface(&self, surface: SurfaceId) {
        let mut queue = self.0.borrow_mut();
        let _newly_forgotten = queue.forgotten.insert(surface);
        let pending = std::mem::take(&mut queue.pending);
        let (kept, dropped_pending): (VecDeque<_>, VecDeque<_>) = pending
            .into_iter()
            .partition(|action| action.surface != surface);
        queue.pending = kept;
        let dropped_watches: Vec<_> = queue
            .watches
            .extract_if(.., |_, action| action.surface == surface)
            .collect();
        (queue.requests).extend(
            dropped_watches
                .iter()
                .map(|(id, _)| WatchRequest::Unwatch(*id)),
        );
        // Dropping an action can run code that queues another action.
        drop(queue);
        drop((dropped_pending, dropped_watches));
    }

    #[must_use]
    pub fn has_pending_for(&self, surface: SurfaceId) -> bool {
        (self.0.borrow().pending.iter()).any(|action| action.surface == surface)
    }

    /// Takes the watch requests made since the last call, for the platform to execute.
    #[must_use]
    pub fn take_requests(&self) -> Vec<WatchRequest> {
        std::mem::take(&mut self.0.borrow_mut().requests)
    }

    /// Queues the action registered for a watch once the platform reports the watched descriptor as readable.
    pub fn handle(&self, event: WatchEvent) {
        let WatchEvent::Readable(id) = event;
        let mut queue = self.0.borrow_mut();
        if let Some(action) = queue.watches.remove(&id) {
            queue.pending.push_back(action);
        }
    }

    /// Drops every queued action, watch and request, and every action queued after that.
    pub fn close(&self) {
        let mut queue = self.0.borrow_mut();
        queue.closed = true;
        let dropped = (
            std::mem::take(&mut queue.pending),
            std::mem::take(&mut queue.watches),
            std::mem::take(&mut queue.requests),
        );
        drop(queue);
        drop(dropped);
    }
}

/// The actions removed from the queue for one frame. Each action runs after the view of the surface it targets is
/// composed.
#[must_use]
pub struct ActionBatch(Vec<PendingAction>);

impl ActionBatch {
    /// Runs the actions of the batch for `surface` on `view`, in the order in which they were queued.
    pub fn run(&mut self, surface: SurfaceId, view: &mut View) {
        for action in self.0.extract_if(.., |action| action.surface == surface) {
            (action.apply)(view);
        }
    }
}

#[cfg(test)]
mod tests;
