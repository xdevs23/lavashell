// SPDX-License-Identifier: GPL-3.0-only
use crate::scene_pipeline::BuiltScene;
use lavashell_actor::{Latest, lock_ignoring_poison};
use lavashell_core::scene::tree::SceneTree;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// The slot with the newest built scene for each surface. The scene builder publishes the scenes, and the platform
/// reads them when it draws.
pub(crate) struct SurfaceSlots {
    slots: Mutex<BTreeMap<SurfaceId, Arc<Latest<BuiltScene>>>>,
    empty: Arc<BuiltScene>,
}

impl Default for SurfaceSlots {
    fn default() -> Self {
        Self {
            slots: Mutex::default(),
            empty: Arc::new(BuiltScene {
                snapshot: SceneTree::default().snapshot(),
                state: SurfaceState::default(),
            }),
        }
    }
}

impl SurfaceSlots {
    /// The newest scene for `surface`, or an empty scene with the default state before the first.
    pub(crate) fn read(&self, surface: SurfaceId) -> Arc<BuiltScene> {
        let slot = self.lock().get(&surface).cloned();
        (slot.and_then(|slot| slot.read())).unwrap_or_else(|| Arc::clone(&self.empty))
    }

    pub(crate) fn publish(&self, surface: SurfaceId, scene: BuiltScene) {
        let slot = Arc::clone(self.lock().entry(surface).or_default());
        slot.publish(scene);
    }

    pub(crate) fn retire(&self, surface: SurfaceId) {
        let retired = self.lock().remove(&surface);
        if let Some(slot) = retired {
            slot.clear();
        }
    }

    /// Each change of the map inserts or removes one slot, so the map is valid after a panic.
    fn lock(&self) -> MutexGuard<'_, BTreeMap<SurfaceId, Arc<Latest<BuiltScene>>>> {
        lock_ignoring_poison(&self.slots)
    }
}
