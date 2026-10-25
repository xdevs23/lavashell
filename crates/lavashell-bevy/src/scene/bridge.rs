// SPDX-License-Identifier: GPL-3.0-only
use crate::vulkan_host::request_external_memory_fd;
use bevy::{prelude::*, render::renderer::raw_vulkan_init::RawVulkanInitSettings};
use lavashell_renderer::ExternalImage;
use std::{
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// A scene image that the producer has finished releasing to the consumer, exported for import by another device.
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

/// Owned by the consumer of a scene image while the consumer reads the image. The lease contains a reference to the
/// image and the `ReturnTarget` of the scene target. Dropping the last reference to the lease returns the scene target
/// to the ring.
pub(super) struct Lease {
    _return: ReturnTarget,
    _image: Arc<lavashell_vulkan::Image>,
}

impl Lease {
    pub fn new(target: ReturnTarget, image: Arc<lavashell_vulkan::Image>) -> Arc<Self> {
        Arc::new(Self {
            _return: target,
            _image: image,
        })
    }
}

/// Returns a scene target to the ring when the `ReturnTarget` is dropped, whether or not the scene in the target
/// reached the consumer.
pub(super) struct ReturnTarget(Arc<TargetState>);

impl ReturnTarget {
    pub fn new(state: Arc<TargetState>) -> Self {
        Self(state)
    }

    /// Marks the target as released to the consumer. The consumer returns the target by dropping the lease.
    pub fn exported(&self) {
        self.0.set(SlotState::Exported);
    }
}

impl Drop for ReturnTarget {
    fn drop(&mut self) {
        self.0.set(SlotState::Free);
    }
}

/// The state of a scene target in the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotState {
    /// The ring can select the target for the next render.
    Free,
    /// Selected for the render of the current frame.
    Rendering,
    /// The scene in the target is released to the consumer, and the consumer has not returned the target yet.
    Exported,
}

/// The state of one scene target, shared by the ring, the export and the lease of the consumer.
pub(super) struct TargetState(Mutex<SlotState>);

impl TargetState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(SlotState::Free)))
    }

    pub fn get(&self) -> SlotState {
        *self.lock()
    }

    pub fn set(&self, state: SlotState) {
        *self.lock() = state;
    }

    /// The state is one plain value, so a panic of a thread that had locked the state leaves the state valid.
    fn lock(&self) -> MutexGuard<'_, SlotState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BridgeStats {
    pub submitted: u64,
    pub completed: u64,
    pub skipped_busy: u64,
    pub skipped_unready: u64,
    pub coalesced: u64,
    pub pending: usize,
}

#[derive(Default)]
struct Output {
    latest: Option<Arc<CompletedFrame>>,
    last_generation: u64,
}

#[derive(Default)]
struct Shared {
    output: Mutex<Output>,
    stats: Mutex<BridgeStats>,
    generation: AtomicU64,
}

/// Publishes the latest completed scene image to the consumer. The image never leaves the GPU. A newer image replaces
/// an image that the consumer has not taken yet.
#[derive(Resource, Clone, Default)]
pub struct SceneBridge(Arc<Shared>);

impl SceneBridge {
    /// Requests the Vulkan support for external memory file descriptors on the device created by Bevy. Call it before
    /// `RenderPlugin` or `DefaultPlugins` creates the device of the producer.
    pub fn configure(&self, app: &mut App) {
        let mut settings = app
            .world_mut()
            .get_resource_or_init::<RawVulkanInitSettings>();
        request_external_memory_fd(&mut settings);
    }

    #[must_use]
    pub fn take(&self) -> Option<Arc<CompletedFrame>> {
        self.output().latest.take()
    }

    #[must_use]
    pub fn latest(&self) -> Option<Arc<CompletedFrame>> {
        self.output().latest.clone()
    }

    /// The mailbox of the latest frame contains plain data, so a panic of a thread that had locked the mailbox leaves
    /// the mailbox valid.
    fn output(&self) -> MutexGuard<'_, Output> {
        self.0.output.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The counters are plain data, so a panic of a thread that had locked the counters leaves the counters
    /// valid.
    fn counters(&self) -> MutexGuard<'_, BridgeStats> {
        self.0.stats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn discard_latest(&self) {
        self.output().latest = None;
    }

    #[must_use]
    pub fn stats(&self) -> BridgeStats {
        *self.counters()
    }

    pub(super) fn generation(&self) -> u64 {
        self.0.generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(super) fn submit(&self) {
        let mut counters = self.counters();
        counters.pending += 1;
        counters.submitted += 1;
    }

    pub(super) fn publish(&self, frame: CompletedFrame) {
        {
            let mut counters = self.counters();
            counters.completed += 1;
            counters.pending -= 1;
        }
        let mut output = self.output();
        if frame.generation > output.last_generation {
            output.last_generation = frame.generation;
            if output.latest.replace(Arc::new(frame)).is_some() {
                self.counters().coalesced += 1;
            }
        }
    }

    pub(super) fn busy(&self) {
        self.counters().skipped_busy += 1;
    }

    pub(super) fn finish_unready(&self) {
        let mut counters = self.counters();
        counters.pending -= 1;
        counters.skipped_unready += 1;
    }

    pub(super) fn unready(&self) {
        self.counters().skipped_unready += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparing_render_is_counted_until_completion_without_becoming_a_presentable_frame() {
        let bridge = SceneBridge::default();
        bridge.submit();
        assert_eq!(bridge.stats().pending, 1);
        assert!(bridge.take().is_none());
        bridge.finish_unready();
        let stats = bridge.stats();
        assert_eq!(stats.pending, 0);
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
        assert_eq!(state.get(), SlotState::Exported);
        drop(consumer_lease);
        assert_eq!(state.get(), SlotState::Free);
    }
}
