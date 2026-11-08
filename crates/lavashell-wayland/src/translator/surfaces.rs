// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;

/// The native surfaces of the surfaces of the runtime. A surface is live from its creation until the runtime destroys
/// it. A destroyed surface is retired: it reports nothing more, and its native surface remains until the release.
pub(crate) struct NativeSurfaces<S> {
    live: BTreeMap<SurfaceId, S>,
    retired: BTreeMap<SurfaceId, S>,
}

impl<S> Default for NativeSurfaces<S> {
    fn default() -> Self {
        Self {
            live: BTreeMap::new(),
            retired: BTreeMap::new(),
        }
    }
}

impl<S> NativeSurfaces<S> {
    /// Adds the native surface of the new surface `id`.
    pub(crate) fn insert(&mut self, id: SurfaceId, surface: S) {
        let _ids_are_never_reused = self.live.insert(id, surface);
    }

    pub(crate) fn live(&self, id: SurfaceId) -> Option<&S> {
        self.live.get(&id)
    }

    pub(crate) fn live_mut(&mut self, id: SurfaceId) -> Option<&mut S> {
        self.live.get_mut(&id)
    }

    pub(crate) fn is_live(&self, id: SurfaceId) -> bool {
        self.live.contains_key(&id)
    }

    /// The id of the live surface for which `matches` returns true.
    pub(crate) fn find_live(&self, matches: impl Fn(&S) -> bool) -> Option<SurfaceId> {
        (self.live.iter()).find_map(|(id, surface)| matches(surface).then_some(*id))
    }

    /// Retires the live surface `id` and returns it.
    pub(crate) fn retire(&mut self, id: SurfaceId) -> Option<&mut S> {
        let surface = self.live.remove(&id)?;
        Some(self.retired.entry(id).or_insert(surface))
    }

    /// Removes the retired surface `id` and returns it.
    pub(crate) fn release(&mut self, id: SurfaceId) -> Option<S> {
        self.retired.remove(&id)
    }

    /// Drops every surface, the live ones first.
    pub(crate) fn clear(&mut self) {
        self.live.clear();
        self.retired.clear();
    }
}

#[cfg(test)]
mod tests;
