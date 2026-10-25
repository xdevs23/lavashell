// SPDX-License-Identifier: GPL-3.0-only
//! Detection of leaves whose packet key repeats from the previous plan. Filling the packet cache costs extra work
//! for a copy that may never be drawn. Only a leaf whose key repeats gets a copy, and a packet seen once is painted
//! directly.

use crate::packet_cache::PacketCacheKey;
use lavashell_core::scene::tree::NodeKey;
use lavashell_core::surface::SurfaceId;
use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
pub(crate) struct RepeatedLeafFilter {
    previous: BTreeMap<(SurfaceId, NodeKey), PacketCacheKey>,
}

impl RepeatedLeafFilter {
    /// Whether the previous plan of the same leaf on the same surface had the same `key`. Records `key` for the next
    /// plan.
    pub fn repeats(&mut self, key: PacketCacheKey) -> bool {
        self.previous.insert((key.surface, key.leaf), key) == Some(key)
    }

    /// Forgets the leaves of `surface` that its last plan did not show.
    pub fn retain_shown(&mut self, surface: SurfaceId, shown: &HashSet<NodeKey>) {
        (self.previous).retain(|(existing, leaf), _| *existing != surface || shown.contains(leaf));
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.previous
            .retain(|(existing, _), _| *existing != surface);
    }
}
