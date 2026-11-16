// SPDX-License-Identifier: GPL-3.0-only

use crate::packet_cache::PacketCacheKey;
use lavashell_core::scene::tree::NodeKey;
use lavashell_core::surface::SurfaceId;
use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
pub(crate) struct RepeatedLeafFilter {
    previous: BTreeMap<(SurfaceId, NodeKey), PacketCacheKey>,
}

impl RepeatedLeafFilter {
    pub fn repeats(&mut self, key: PacketCacheKey) -> bool {
        self.previous.insert((key.surface, key.leaf), key) == Some(key)
    }

    pub fn retain_shown(&mut self, surface: SurfaceId, shown: &HashSet<NodeKey>) {
        (self.previous).retain(|(existing, leaf), _| *existing != surface || shown.contains(leaf));
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.previous
            .retain(|(existing, _), _| *existing != surface);
    }
}
