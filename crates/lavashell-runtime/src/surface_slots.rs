// SPDX-License-Identifier: GPL-3.0-only
use crate::FrameSequence;
use lavashell_actor::{Latest, lock_ignoring_poison};
use lavashell_core::scene::tree::{SceneSnapshot, SceneTree};
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// A scene that the scene builder built for a surface.
pub struct BuiltScene {
    /// The sequence of the newest composed frame in the scene. A scene built only from frames sent outside a compose
    /// has no sequence.
    pub sequence: Option<FrameSequence>,
    pub snapshot: SceneSnapshot,
}

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
                sequence: None,
                snapshot: SceneTree::default().snapshot(),
            }),
        }
    }
}

impl SurfaceSlots {
    /// The newest scene for `surface`, or an empty scene before the first.
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
