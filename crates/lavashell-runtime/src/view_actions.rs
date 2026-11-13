// SPDX-License-Identifier: GPL-3.0-only
use crate::watches::{self, AfterReady, Watches};
use lavashell_core::surface::SurfaceId;
use lavashell_core::view::View;
use lavashell_platform::{Readiness, WatchId};
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

struct Queue {
    pending: VecDeque<PendingAction>,
    /// The actions to queue once the descriptor watched under the same id is readable.
    waiting: BTreeMap<WatchId, PendingAction>,
    watches: Watches,
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
    /// Fails when the watches of the run closed, or when the number of the new watch does not fit a watch id.
    pub fn dispatch_when_readable(
        &self,
        source: OwnedFd,
        surface: SurfaceId,
        apply: impl FnOnce(&mut View) + 'static,
    ) -> Result<(), watches::Error> {
        let mut queue = self.0.borrow_mut();
        if !queue.accepts(surface) {
            return Ok(());
        }
        let shared = Rc::downgrade(&self.0);
        let id = queue
            .watches
            .watch(source, Readiness::Readable, move |id| {
                if let Some(shared) = shared.upgrade() {
                    let mut queue = shared.borrow_mut();
                    if let Some(action) = queue.waiting.remove(&id) {
                        queue.pending.push_back(action);
                    }
                }
                AfterReady::End
            })?;
        let _no_watch_had_the_id = queue.waiting.insert(id, PendingAction::new(surface, apply));
        Ok(())
    }
}

/// A queue of actions. Each action changes the view of one surface. Controls and asynchronous transfer callbacks add
/// actions to it through `ViewActions`. The runtime runs each action after it composes the view of that surface.
pub struct ActionQueue(Rc<RefCell<Queue>>);

impl ActionQueue {
    /// A queue whose actions wait for descriptors watched through `watches`.
    #[must_use]
    pub fn new(watches: &Watches) -> Self {
        Self(Rc::new(RefCell::new(Queue {
            pending: VecDeque::new(),
            waiting: BTreeMap::new(),
            watches: watches.clone(),
            forgotten: BTreeSet::new(),
            closed: false,
        })))
    }

    /// A handle that adds actions to this queue.
    #[must_use]
    pub fn view_actions(&self) -> ViewActions {
        ViewActions(self.0.clone())
    }

    /// Removes the queued actions as a batch. The actions queued while the batch runs wait for the next batch.
    pub(crate) fn take_batch(&self) -> ActionBatch {
        ActionBatch(self.0.borrow_mut().pending.drain(..).collect())
    }

    /// Puts the actions of `batch` that did not run back at the front of the queue, in the order in which they were
    /// queued. An action for a forgotten surface is dropped.
    pub(crate) fn requeue(&self, batch: ActionBatch) {
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
        let dropped_waiting: Vec<_> = queue
            .waiting
            .extract_if(.., |_, action| action.surface == surface)
            .collect();
        let watches = queue.watches.clone();
        // Dropping an action can run code that queues another action.
        drop(queue);
        for (id, _) in &dropped_waiting {
            watches.unwatch(*id);
        }
        drop((dropped_pending, dropped_waiting));
    }

    #[must_use]
    pub fn has_pending_for(&self, surface: SurfaceId) -> bool {
        (self.0.borrow().pending.iter()).any(|action| action.surface == surface)
    }

    /// Drops every queued and waiting action, and every action queued after that.
    pub fn close(&self) {
        let mut queue = self.0.borrow_mut();
        queue.closed = true;
        let dropped = (
            std::mem::take(&mut queue.pending),
            std::mem::take(&mut queue.waiting),
        );
        drop(queue);
        drop(dropped);
    }
}

/// The actions removed from the queue for one frame. Each action runs after the view of the surface it targets is
/// composed.
#[must_use]
pub(crate) struct ActionBatch(Vec<PendingAction>);

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
