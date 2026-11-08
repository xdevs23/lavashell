// SPDX-License-Identifier: GPL-3.0-only
use crate::translator::ViewLink;
use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
use lavashell_actor::Latest;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::AccessibilityEvent;
use std::sync::Arc;

pub(super) type SharedSnapshot = Arc<Latest<TreeUpdate>>;

pub(super) struct Activation {
    surface: SurfaceId,
    view: ViewLink,
    snapshot: SharedSnapshot,
}

impl Activation {
    pub(super) fn new(surface: SurfaceId, view: ViewLink, snapshot: SharedSnapshot) -> Self {
        Self {
            surface,
            view,
            snapshot,
        }
    }
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let snapshot = self.snapshot.read().map(Arc::unwrap_or_clone);
        self.view.report(AccessibilityEvent::Activate(self.surface));
        snapshot
    }
}

pub(super) struct Actions {
    surface: SurfaceId,
    view: ViewLink,
}

impl Actions {
    pub(super) fn new(surface: SurfaceId, view: ViewLink) -> Self {
        Self { surface, view }
    }
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        self.view
            .report(AccessibilityEvent::Action(self.surface, request));
    }
}

pub(super) struct Deactivation {
    surface: SurfaceId,
    view: ViewLink,
    snapshot: SharedSnapshot,
}

impl Deactivation {
    pub(super) fn new(surface: SurfaceId, view: ViewLink, snapshot: SharedSnapshot) -> Self {
        Self {
            surface,
            view,
            snapshot,
        }
    }
}

impl DeactivationHandler for Deactivation {
    /// Drops the tree stored for the assistive technologies before the view learns that none reads the surface.
    fn deactivate_accessibility(&mut self) {
        self.snapshot.clear();
        self.view
            .report(AccessibilityEvent::Deactivate(self.surface));
    }
}
