// SPDX-License-Identifier: GPL-3.0-only
use super::{Namespaces, NodeKey, Presented, SceneTree};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

/// The nodes of a scene tree at the moment the snapshot was taken. The snapshot shares each node with the tree until
/// the tree changes that node. Creating a snapshot copies zero nodes.
#[derive(Clone)]
pub struct SceneSnapshot {
    namespaces: Namespaces,
    moving: Arc<BTreeSet<NodeKey>>,
}

// A snapshot and a presentation of it are read on a different thread than the one that changes the tree it was
// taken from.
const _: () = {
    const fn shareable<T: Send + Sync>() {}
    shareable::<SceneSnapshot>();
    shareable::<Presented<'static>>();
};

impl SceneTree {
    #[must_use]
    pub fn snapshot(&self) -> SceneSnapshot {
        SceneSnapshot {
            namespaces: self.namespaces.clone(),
            moving: Arc::clone(&self.moving),
        }
    }
}

impl SceneSnapshot {
    /// The snapshot with its animated slots sampled at `now`, measured from the same epoch as the time passed to
    /// `View::frame_into_at`.
    #[must_use]
    pub fn at(&self, now: Duration) -> Presented<'_> {
        self.namespaces.at(now)
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    /// The earliest time, at or after `now`, at which an animated group of the snapshot changes.
    #[must_use]
    pub(crate) fn next_change(&self, now: Duration) -> Option<Duration> {
        self.namespaces.next_change(&self.moving, now)
    }
}
