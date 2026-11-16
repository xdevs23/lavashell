// SPDX-License-Identifier: GPL-3.0-only
use crate::scheduler::FrameKind;
use crate::surface_slots::BuiltSurface;
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub(super) struct FramePacing {
    started_frames: BTreeMap<SurfaceId, (FrameKind, FrameTime)>,
    state_commits: BTreeMap<SurfaceId, Arc<BuiltSurface>>,
    scene_deadline: Option<Duration>,
    scheduler_wake: Option<Duration>,
    last_tick: Duration,
}

impl FramePacing {
    pub(super) fn start_frame(&mut self, surface: SurfaceId, kind: FrameKind, time: FrameTime) {
        let _older_frame = self.started_frames.insert(surface, (kind, time));
    }

    pub(super) fn commit_state(&mut self, surface: SurfaceId, built: Arc<BuiltSurface>) {
        let _replaced = self.state_commits.insert(surface, built);
    }

    pub(super) fn set_scene_deadline(&mut self, deadline: Option<Duration>) {
        self.scene_deadline = deadline;
    }

    pub(super) fn set_scheduler_wake(&mut self, wake: Option<Duration>) {
        self.scheduler_wake = wake;
    }

    pub(super) fn take_started_frames(&mut self) -> BTreeMap<SurfaceId, (FrameKind, FrameTime)> {
        mem::take(&mut self.started_frames)
    }

    pub(super) fn take_state_commits(&mut self) -> BTreeMap<SurfaceId, Arc<BuiltSurface>> {
        mem::take(&mut self.state_commits)
    }

    pub(super) fn scene_due(&self, now: Duration) -> bool {
        (self.scene_deadline).is_some_and(|deadline| deadline <= now)
    }

    pub(super) fn tick(&mut self, now: Duration) -> Duration {
        now.saturating_sub(mem::replace(&mut self.last_tick, now))
    }

    pub(super) fn forget(&mut self, surface: SurfaceId) {
        let _undrawn_frame = self.started_frames.remove(&surface);
        let _uncommitted_state = self.state_commits.remove(&surface);
    }

    pub(super) fn take_deadline(&mut self, now: Duration) -> Option<Duration> {
        let scene = self.scene_deadline.filter(|&deadline| deadline > now);
        let wake = self.scheduler_wake;
        if wake.is_some_and(|wake| wake <= now) {
            self.scheduler_wake = None;
        }
        scene.into_iter().chain(wake).min()
    }
}
