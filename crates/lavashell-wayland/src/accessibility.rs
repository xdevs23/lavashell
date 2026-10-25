// SPDX-License-Identifier: GPL-3.0-only
//! Publishes the accessibility tree of each surface to the Linux accessibility bus through `accesskit_unix`, and turns
//! the actions requested by assistive technologies into Lavashell action requests. The module does not depend on
//! composition or GPU submission.

mod actions;
mod handlers;
mod ids;
mod mapping;
mod properties;
#[cfg(test)]
mod tests;
mod text;

use accesskit::TreeUpdate;
use accesskit_unix::Adapter;
use handlers::{Actions, Activation, Deactivation, SharedSnapshot, lock_snapshot};
use lavashell_core::accessibility::{AccessibilityTree, ActionRequest};
use sctk::reexports::calloop::channel::Sender;
use smithay_client_toolkit as sctk;
use std::sync::{Arc, Mutex};

pub(crate) enum Event {
    Activate(usize),
    Action(usize, accesskit::ActionRequest),
    Deactivate(usize),
}

pub(crate) struct Bridge {
    adapter: Adapter,
    shared: SharedSnapshot,
    ids: ids::Ids,
    previous: Option<TreeUpdate>,
    revision: Option<u64>,
    title: String,
    active: bool,
}

impl Bridge {
    pub(crate) fn new(surface: usize, title: String, events: Sender<Event>) -> Self {
        let shared = Arc::new(Mutex::new(None));
        let adapter = Adapter::new(
            Activation::new(surface, events.clone(), shared.clone()),
            Actions::new(surface, events.clone()),
            Deactivation::new(surface, events),
        );
        Self {
            adapter,
            shared,
            ids: ids::Ids::default(),
            previous: None,
            revision: None,
            title,
            active: false,
        }
    }

    pub(crate) fn window_focus(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    pub(crate) fn publish(&mut self, tree: &AccessibilityTree, force: bool) {
        if !self.active || tree.nodes.is_empty() || !force && self.revision == Some(tree.revision) {
            return;
        }
        let full = mapping::tree(tree, &mut self.ids, &self.title);
        // The activation handler runs on an AccessKit thread and reads only this snapshot. The application callbacks
        // run on the UI thread, which receives the activation through the event channel.
        *lock_snapshot(&self.shared) = Some(full.clone());
        let update = if force {
            full.clone()
        } else {
            mapping::difference(&full, self.previous.as_ref())
        };
        self.adapter.update_if_active(|| update);
        self.previous = Some(full);
        self.revision = Some(tree.revision);
    }

    pub(crate) fn action(
        &self,
        request: accesskit::ActionRequest,
        tree: &AccessibilityTree,
    ) -> Option<ActionRequest> {
        if !self.active {
            return None;
        }
        actions::request(request, tree, &self.ids)
    }

    pub(crate) fn deactivate(&mut self) {
        self.active = false;
        *lock_snapshot(&self.shared) = None;
        self.previous = None;
        self.revision = None;
    }

    pub(crate) fn activate(&mut self) {
        self.active = true;
    }
}
