// SPDX-License-Identifier: GPL-3.0-only
use super::{Namespaces, NodeKey, Presented, SceneTree, TreeId};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct SceneSnapshot {
    lineage: TreeId,
    namespaces: Namespaces,
    moving: Arc<BTreeSet<NodeKey>>,
}

const _: () = {
    const fn shareable<T: Send + Sync>() {}
    shareable::<SceneSnapshot>();
    shareable::<Presented<'static>>();
};

impl SceneTree {
    #[must_use]
    pub fn snapshot(&self) -> SceneSnapshot {
        SceneSnapshot {
            lineage: self.id,
            namespaces: self.namespaces.clone(),
            moving: Arc::clone(&self.moving),
        }
    }
}

impl SceneSnapshot {
    #[must_use]
    pub fn at(&self, now: Duration) -> Presented<'_> {
        self.namespaces.at(now)
    }

    #[must_use]
    pub fn lineage(&self) -> TreeId {
        self.lineage
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }

    #[must_use]
    pub fn next_change(&self, now: Duration) -> Option<Duration> {
        self.namespaces.next_change(&self.moving, now)
    }
}
