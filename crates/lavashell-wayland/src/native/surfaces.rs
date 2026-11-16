// SPDX-License-Identifier: GPL-3.0-only
use super::Surface;
use crate::accessibility::Bridge;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct NativeSurfaces {
    live: BTreeMap<SurfaceId, Surface>,
    retired: BTreeMap<SurfaceId, Surface>,
}

impl NativeSurfaces {
    pub(crate) fn insert(&mut self, id: SurfaceId, surface: Surface) {
        let _ids_are_never_reused = self.live.insert(id, surface);
    }

    pub(crate) fn live(&self, id: SurfaceId) -> Option<&Surface> {
        self.live.get(&id)
    }

    pub(crate) fn bridge_mut(&mut self, id: SurfaceId) -> Option<&mut Bridge> {
        self.live.get_mut(&id)?.accessibility.as_mut()
    }

    pub(crate) fn is_live(&self, id: SurfaceId) -> bool {
        self.live.contains_key(&id)
    }

    pub(crate) fn find_live(&self, matches: impl Fn(&Surface) -> bool) -> Option<SurfaceId> {
        (self.live.iter()).find_map(|(id, surface)| matches(surface).then_some(*id))
    }

    pub(crate) fn retire(&mut self, id: SurfaceId) {
        if let Some(mut surface) = self.live.remove(&id) {
            surface.accessibility = None;
            let _ids_are_never_reused = self.retired.insert(id, surface);
        }
    }

    pub(crate) fn release(&mut self, id: SurfaceId) -> Option<Surface> {
        self.retired.remove(&id)
    }

    pub(crate) fn clear(&mut self) {
        self.live.clear();
        self.retired.clear();
    }
}

#[cfg(test)]
mod tests;
