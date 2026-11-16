// SPDX-License-Identifier: GPL-3.0-only

mod handlers;
#[cfg(test)]
pub(crate) mod test_error;
#[cfg(test)]
mod tests;

use crate::translator::ViewLink;
use accesskit::TreeUpdate;
use accesskit_unix::Adapter;
use handlers::{Actions, Activation, Deactivation, SharedSnapshot};
use lavashell_actor::Latest;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::tree_difference;
use std::sync::Arc;

pub(crate) struct Bridge {
    adapter: Adapter,
    shared: SharedSnapshot,
}

impl Bridge {
    pub(crate) fn new(surface: SurfaceId, view: ViewLink) -> Self {
        let shared = Arc::new(Latest::new());
        let adapter = Adapter::new(
            Activation::new(surface, view.clone(), shared.clone()),
            Actions::new(surface, view.clone()),
            Deactivation::new(surface, view, shared.clone()),
        );
        Self { adapter, shared }
    }

    pub(crate) fn window_focus(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    pub(crate) fn publish(&mut self, tree: TreeUpdate) {
        let tree = Arc::new(tree);
        let previous = self.shared.read();
        self.shared.publish_shared(tree.clone());
        (self.adapter).update_if_active(|| tree_difference(&tree, previous.as_deref()));
    }
}
