// SPDX-License-Identifier: GPL-3.0-only
use super::{Namespaces, Presented, SceneTree};
use std::time::Duration;

/// The nodes of a scene tree at the moment the snapshot was taken. The snapshot shares each node with the tree until
/// the tree changes that node. Creating a snapshot copies zero nodes.
#[derive(Clone)]
pub struct SceneSnapshot(Namespaces);

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
        SceneSnapshot(self.namespaces.clone())
    }
}

impl SceneSnapshot {
    /// The snapshot with its animated slots sampled at `now`, measured from the same epoch as the time passed to
    /// `View::frame_at`.
    #[must_use]
    pub fn at(&self, now: Duration) -> Presented<'_> {
        self.0.at(now)
    }
}
