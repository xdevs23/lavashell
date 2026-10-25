// SPDX-License-Identifier: GPL-3.0-only
use super::Event;
use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
use sctk::reexports::calloop::channel::Sender;
use smithay_client_toolkit as sctk;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub(super) type SharedSnapshot = Arc<Mutex<Option<TreeUpdate>>>;

/// Ignores lock poisoning, because the snapshot is replaced as a whole and is valid after a panic.
pub(super) fn lock_snapshot(snapshot: &SharedSnapshot) -> MutexGuard<'_, Option<TreeUpdate>> {
    snapshot.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) struct Activation {
    surface: usize,
    events: Sender<Event>,
    snapshot: SharedSnapshot,
}

impl Activation {
    pub(super) fn new(surface: usize, events: Sender<Event>, snapshot: SharedSnapshot) -> Self {
        Self {
            surface,
            events,
            snapshot,
        }
    }
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let snapshot = lock_snapshot(&self.snapshot).clone();
        // A channel event wakes calloop even when no visual frame or scene frame is due.
        if self.events.send(Event::Activate(self.surface)).is_err() {
            log::debug!("an accessibility request arrived after the app closed");
        }
        snapshot
    }
}

pub(super) struct Actions {
    surface: usize,
    events: Sender<Event>,
}

impl Actions {
    pub(super) fn new(surface: usize, events: Sender<Event>) -> Self {
        Self { surface, events }
    }
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if self
            .events
            .send(Event::Action(self.surface, request))
            .is_err()
        {
            log::debug!("an accessibility request arrived after the app closed");
        }
    }
}

pub(super) struct Deactivation {
    surface: usize,
    events: Sender<Event>,
}

impl Deactivation {
    pub(super) fn new(surface: usize, events: Sender<Event>) -> Self {
        Self { surface, events }
    }
}

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        if self.events.send(Event::Deactivate(self.surface)).is_err() {
            log::debug!("an accessibility request arrived after the app closed");
        }
    }
}
