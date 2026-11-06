// SPDX-License-Identifier: GPL-3.0-only
//! Publishes the accessibility tree that the runtime encodes for each surface to the Linux accessibility bus through
//! `accesskit_unix`, and reports what assistive technologies do as accessibility events. The module needs only the
//! runtime requests and the AccessKit adapter.

mod handlers;
#[cfg(test)]
mod tests;

use accesskit::TreeUpdate;
use accesskit_unix::Adapter;
use handlers::{Actions, Activation, Deactivation, SharedSnapshot};
use lavashell_actor::Latest;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{AccessibilityEvent, tree_difference};
use sctk::reexports::calloop::channel::Sender;
use smithay_client_toolkit as sctk;
use std::sync::Arc;

pub(crate) struct Bridge {
    adapter: Adapter,
    shared: SharedSnapshot,
}

impl Bridge {
    pub(crate) fn new(surface: SurfaceId, events: Sender<AccessibilityEvent>) -> Self {
        let shared = Arc::new(Latest::new());
        let adapter = Adapter::new(
            Activation::new(surface, events.clone(), shared.clone()),
            Actions::new(surface, events.clone()),
            Deactivation::new(surface, events),
        );
        Self { adapter, shared }
    }

    pub(crate) fn window_focus(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    /// Stores `tree` for an assistive technology that starts reading the surface later, and applies the nodes that
    /// changed since the stored tree to the tree read by the assistive technologies.
    pub(crate) fn publish(&mut self, tree: TreeUpdate) {
        let update = tree_difference(&tree, self.shared.read().as_deref());
        // The activation handler runs on an AccessKit thread and reads only this snapshot. The application callbacks
        // run on the UI thread, which receives the activation through the event channel.
        self.shared.publish(tree);
        self.adapter.update_if_active(|| update);
    }

    /// Drops the stored tree when the last assistive technology stops reading the surface.
    pub(crate) fn deactivate(&self) {
        self.shared.clear();
    }
}
