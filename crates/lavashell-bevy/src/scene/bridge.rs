// SPDX-License-Identifier: GPL-3.0-only
use bevy::prelude::*;
use lavashell_actor::{Latest, Publication, lock_ignoring_poison};
use lavashell_vulkan_renderer::ExternalImage;
use lavashell_vulkan_renderer::vulkan::SceneLease;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

pub struct CompletedFrame {
    pub generation: u64,
    pub image: ExternalImage,
    pub produced_at: Instant,
    pub completed_at: Instant,
}

impl CompletedFrame {
    pub(super) fn new(generation: u64, image: ExternalImage, at: Instant) -> Self {
        Self {
            generation,
            image,
            produced_at: at,
            completed_at: Instant::now(),
        }
    }
}

pub(super) struct Lease {
    _return: ReturnTarget,
    _image: Arc<lavashell_vulkan::Image>,
}

impl SceneLease for Lease {}

impl Lease {
    pub fn new(target: ReturnTarget, image: Arc<lavashell_vulkan::Image>) -> Arc<Self> {
        Arc::new(Self {
            _return: target,
            _image: image,
        })
    }
}

pub(super) struct ReturnTarget(Arc<TargetState>);

impl ReturnTarget {
    pub fn new(state: Arc<TargetState>) -> Self {
        Self(state)
    }

    pub fn exported(&self) {
        self.0.set(TargetPhase::Exported);
    }
}

impl Drop for ReturnTarget {
    fn drop(&mut self) {
        self.0.set(TargetPhase::Free);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TargetPhase {
    Free,
    Rendering,
    Exported,
}

pub(super) struct TargetState(Mutex<TargetPhase>);

impl TargetState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(TargetPhase::Free)))
    }

    pub fn get(&self) -> TargetPhase {
        locked(&self.0, |phase| *phase)
    }

    pub fn set(&self, phase: TargetPhase) {
        locked(&self.0, |current| *current = phase);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BridgeStats {
    pub submitted: u64,
    pub completed: u64,
    pub skipped_busy: u64,
    pub skipped_unready: u64,
    pub coalesced: u64,
}

#[derive(Default)]
struct Shared {
    output: Latest<CompletedFrame>,
    stats: Mutex<BridgeStats>,
    exports_in_flight: Mutex<usize>,
    generation: AtomicU64,
}

#[derive(Resource, Clone, Default)]
pub struct SceneBridge(Arc<Shared>);

impl SceneBridge {
    #[must_use]
    pub fn take(&self) -> Option<Arc<CompletedFrame>> {
        self.0.output.take()
    }

    #[must_use]
    pub fn latest(&self) -> Option<Arc<CompletedFrame>> {
        self.0.output.read()
    }

    pub(super) fn discard_latest(&self) {
        self.0.output.clear();
    }

    #[must_use]
    pub fn stats(&self) -> BridgeStats {
        locked(&self.0.stats, |stats| *stats)
    }

    pub(super) fn has_export_in_flight(&self) -> bool {
        locked(&self.0.exports_in_flight, |count| *count > 0)
    }

    pub(super) fn next_generation(&self) -> u64 {
        self.0.generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(super) fn begin_export(&self) {
        locked(&self.0.exports_in_flight, |count| *count += 1);
        self.update_stats(|stats| stats.submitted += 1);
    }

    pub(super) fn publish(&self, frame: CompletedFrame) {
        self.end_export();
        self.update_stats(|stats| stats.completed += 1);
        let published = self.0.output.publish_newer(frame.generation, frame);
        if matches!(published, Publication::Replaced(_)) {
            self.update_stats(|stats| stats.coalesced += 1);
        }
    }

    pub(super) fn finish_export_unpublished(&self) {
        self.end_export();
        self.count_skipped_unready();
    }

    pub(super) fn count_skipped_busy(&self) {
        self.update_stats(|stats| stats.skipped_busy += 1);
    }

    pub(super) fn count_skipped_unready(&self) {
        self.update_stats(|stats| stats.skipped_unready += 1);
    }

    fn end_export(&self) {
        locked(&self.0.exports_in_flight, |count| *count -= 1);
    }

    fn update_stats(&self, update: impl FnOnce(&mut BridgeStats)) {
        locked(&self.0.stats, update);
    }
}

fn locked<T, R>(mutex: &Mutex<T>, use_value: impl FnOnce(&mut T) -> R) -> R {
    use_value(&mut lock_ignoring_poison(mutex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparing_render_is_counted_until_completion_without_becoming_a_presentable_frame() {
        let bridge = SceneBridge::default();
        bridge.begin_export();
        assert!(bridge.has_export_in_flight());
        assert!(bridge.take().is_none());
        bridge.finish_export_unpublished();
        let stats = bridge.stats();
        assert!(!bridge.has_export_in_flight());
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.skipped_unready, 1);
        assert!(bridge.take().is_none());
    }

    #[test]
    fn an_exported_target_returns_only_after_the_last_lease_drops() {
        let state = TargetState::new();
        let returned = ReturnTarget::new(state.clone());
        returned.exported();
        let lease = Arc::new(returned);
        let consumer_lease = lease.clone();
        drop(lease);
        assert_eq!(state.get(), TargetPhase::Exported);
        drop(consumer_lease);
        assert_eq!(state.get(), TargetPhase::Free);
    }
}
