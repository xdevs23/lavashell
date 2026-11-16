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
    waiting: BTreeMap<WatchId, PendingAction>,
    watches: Watches,
    open: BTreeSet<SurfaceId>,
}

impl Queue {
    fn accepts(&self, surface: SurfaceId) -> bool {
        self.open.contains(&surface)
    }
}

#[derive(Clone)]
pub struct ViewActions(Rc<RefCell<Queue>>);

impl ViewActions {
    pub fn dispatch(&self, surface: SurfaceId, apply: impl FnOnce(&mut View) + 'static) {
        let mut queue = self.0.borrow_mut();
        if queue.accepts(surface) {
            queue.pending.push_back(PendingAction::new(surface, apply));
        }
    }

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

pub struct ActionQueue(Rc<RefCell<Queue>>);

impl ActionQueue {
    #[must_use]
    pub fn new(watches: &Watches) -> Self {
        Self(Rc::new(RefCell::new(Queue {
            pending: VecDeque::new(),
            waiting: BTreeMap::new(),
            watches: watches.clone(),
            open: BTreeSet::new(),
        })))
    }

    #[must_use]
    pub fn view_actions(&self) -> ViewActions {
        ViewActions(self.0.clone())
    }

    pub(crate) fn take_batch(&self) -> ActionBatch {
        ActionBatch(self.0.borrow_mut().pending.drain(..).collect())
    }

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

    pub(crate) fn open_surface(&self, surface: SurfaceId) {
        let _newly_open = self.0.borrow_mut().open.insert(surface);
    }

    pub fn forget_surface(&self, surface: SurfaceId) {
        let mut queue = self.0.borrow_mut();
        let _was_open = queue.open.remove(&surface);
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

    pub fn close(&self) {
        let mut queue = self.0.borrow_mut();
        let dropped = (
            std::mem::take(&mut queue.open),
            std::mem::take(&mut queue.pending),
            std::mem::take(&mut queue.waiting),
        );
        drop(queue);
        drop(dropped);
    }
}

#[must_use]
pub(crate) struct ActionBatch(Vec<PendingAction>);

impl ActionBatch {
    pub fn run(&mut self, surface: SurfaceId, view: &mut View) {
        for action in self.0.extract_if(.., |action| action.surface == surface) {
            (action.apply)(view);
        }
    }
}

#[cfg(test)]
mod tests;
