// SPDX-License-Identifier: GPL-3.0-only
use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
use lavashell_actor::Latest;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::AccessibilityEvent;
use sctk::reexports::calloop::channel::Sender;
use smithay_client_toolkit as sctk;
use std::sync::Arc;

pub(super) type SharedSnapshot = Arc<Latest<TreeUpdate>>;

pub(super) struct Activation {
    surface: SurfaceId,
    events: Sender<AccessibilityEvent>,
    snapshot: SharedSnapshot,
}

impl Activation {
    pub(super) fn new(
        surface: SurfaceId,
        events: Sender<AccessibilityEvent>,
        snapshot: SharedSnapshot,
    ) -> Self {
        Self {
            surface,
            events,
            snapshot,
        }
    }
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let snapshot = self.snapshot.read().map(Arc::unwrap_or_clone);
        // A channel event wakes calloop even when no visual frame or scene frame is due.
        let activated = AccessibilityEvent::Activate(self.surface);
        if self.events.send(activated).is_err() {
            log::debug!("an accessibility request arrived after the app closed");
        }
        snapshot
    }
}

pub(super) struct Actions {
    surface: SurfaceId,
    events: Sender<AccessibilityEvent>,
}

impl Actions {
    pub(super) fn new(surface: SurfaceId, events: Sender<AccessibilityEvent>) -> Self {
        Self { surface, events }
    }
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if self
            .events
            .send(AccessibilityEvent::Action(self.surface, request))
            .is_err()
        {
            log::debug!("an accessibility request arrived after the app closed");
        }
    }
}

pub(super) struct Deactivation {
    surface: SurfaceId,
    events: Sender<AccessibilityEvent>,
}

impl Deactivation {
    pub(super) fn new(surface: SurfaceId, events: Sender<AccessibilityEvent>) -> Self {
        Self { surface, events }
    }
}

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        let deactivated = AccessibilityEvent::Deactivate(self.surface);
        if self.events.send(deactivated).is_err() {
            log::debug!("an accessibility request arrived after the app closed");
        }
    }
}
