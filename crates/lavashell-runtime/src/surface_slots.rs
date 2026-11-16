// SPDX-License-Identifier: GPL-3.0-only
use crate::scheduler::FrameSequence;
use lavashell_actor::{Latest, lock_ignoring_poison};
use lavashell_core::scene::tree::{SceneSnapshot, SceneTree};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct BuiltSurface {
    pub sequence: Option<FrameSequence>,
    pub scene: SceneSnapshot,
    pub state: SurfaceState,
}

pub(crate) struct SurfaceSlots {
    slots: Mutex<BTreeMap<SurfaceId, Arc<Latest<BuiltSurface>>>>,
    empty: Arc<BuiltSurface>,
}

impl Default for SurfaceSlots {
    fn default() -> Self {
        Self {
            slots: Mutex::default(),
            empty: Arc::new(BuiltSurface {
                sequence: None,
                scene: SceneTree::default().snapshot(),
                state: SurfaceState::default(),
            }),
        }
    }
}

impl SurfaceSlots {
    pub(crate) fn read(&self, surface: SurfaceId) -> Arc<BuiltSurface> {
        let slot = self.lock().get(&surface).cloned();
        (slot.and_then(|slot| slot.read())).unwrap_or_else(|| Arc::clone(&self.empty))
    }

    pub(crate) fn publish(&self, surface: SurfaceId, built: BuiltSurface) {
        let slot = Arc::clone(self.lock().entry(surface).or_default());
        slot.publish(built);
    }

    pub(crate) fn retire(&self, surface: SurfaceId) {
        let retired = self.lock().remove(&surface);
        if let Some(slot) = retired {
            slot.clear();
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<SurfaceId, Arc<Latest<BuiltSurface>>>> {
        lock_ignoring_poison(&self.slots)
    }
}
