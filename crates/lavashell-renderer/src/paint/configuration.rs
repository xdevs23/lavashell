// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Configuration {
    revision: u64,
    painted: BTreeMap<SurfaceId, u64>,
}

impl Configuration {
    pub fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
        self.painted.clear();
    }

    pub fn needs_reconstruction(&self, surface: SurfaceId) -> bool {
        self.revision != 0 && self.painted.get(&surface).copied() != Some(self.revision)
    }

    pub fn painted(&mut self, surface: SurfaceId) {
        let _earlier_revision = self.painted.insert(surface, self.revision);
    }

    pub fn forget(&mut self, surface: SurfaceId) {
        let _forgotten_revision = self.painted.remove(&surface);
    }

    pub fn clear(&mut self) {
        self.painted.clear();
    }
}

impl crate::Painter {
    pub(crate) fn invalidate_paint_configuration(&mut self) {
        self.clear_packet_cache();
        self.invalidate_effect_snapshots();
        self.paint_configuration.invalidate();
    }
}
