// SPDX-License-Identifier: GPL-3.0-only
use crate::{BuiltSurface, FrameKind};
use lavashell_core::frame::FrameTime;
use lavashell_core::surface::SurfaceId;
use std::collections::BTreeMap;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The frames that the next render runs, and the times at which the views take a turn without an input.
#[derive(Default)]
pub(super) struct FramePacing {
    /// The newest frame started by the scheduler for each surface, until the next render composes or draws it.
    started_frames: BTreeMap<SurfaceId, (FrameKind, FrameTime)>,
    /// The built state that the next render commits without a frame for each surface. The state is read when the
    /// frame scheduler starts the commit, while no frame of the surface is composed or drawn.
    state_commits: BTreeMap<SurfaceId, Arc<BuiltSurface>>,
    /// The instant at which the scene source on the renderer thread produces the next image, as the renderer
    /// reported last.
    scene_deadline: Option<Instant>,
    /// The time at which the scheduler asks the views to report again what they need.
    scheduler_wake: Option<Duration>,
    last_tick: Duration,
}

impl FramePacing {
    /// Records the frame of `kind` that the scheduler started for `surface` at `time`, in place of an older one.
    pub(super) fn start_frame(&mut self, surface: SurfaceId, kind: FrameKind, time: FrameTime) {
        let _older_frame = self.started_frames.insert(surface, (kind, time));
    }

    /// Records `built` as the state that the next render commits without a frame for `surface`.
    pub(super) fn commit_state(&mut self, surface: SurfaceId, built: Arc<BuiltSurface>) {
        let _replaced = self.state_commits.insert(surface, built);
    }

    pub(super) fn set_scene_deadline(&mut self, deadline: Option<Instant>) {
        self.scene_deadline = deadline;
    }

    pub(super) fn set_scheduler_wake(&mut self, wake: Option<Duration>) {
        self.scheduler_wake = wake;
    }

    /// Removes and returns the started frames.
    pub(super) fn take_started_frames(&mut self) -> BTreeMap<SurfaceId, (FrameKind, FrameTime)> {
        mem::take(&mut self.started_frames)
    }

    /// Removes and returns the states to commit without a frame.
    pub(super) fn take_state_commits(&mut self) -> BTreeMap<SurfaceId, Arc<BuiltSurface>> {
        mem::take(&mut self.state_commits)
    }

    /// Whether the scene source is due to produce its next image.
    pub(super) fn scene_due(&self) -> bool {
        (self.scene_deadline).is_some_and(|deadline| deadline <= Instant::now())
    }

    /// Returns the time since the tick before, and makes `now` the time of the last tick.
    pub(super) fn tick(&mut self, now: Duration) -> Duration {
        now.saturating_sub(mem::replace(&mut self.last_tick, now))
    }

    /// Drops the started frame and the state to commit of `surface`.
    pub(super) fn forget(&mut self, surface: SurfaceId) {
        let _undrawn_frame = self.started_frames.remove(&surface);
        let _uncommitted_state = self.state_commits.remove(&surface);
    }

    /// The earlier of the next scene image and the time at which the scheduler asks the views to report again,
    /// measured from `start`, or `None` without either. A scene deadline that has passed is left out, because the
    /// render before this wait asked the scene source to advance, and the renderer reports the next deadline once it
    /// changes. A wake time that is due ends the wait at once and is used up by the next render.
    pub(super) fn deadline(&mut self, start: Instant, now: Instant) -> Option<Instant> {
        let scene = self.scene_deadline.filter(|&deadline| deadline > now);
        let wake = (self.scheduler_wake).and_then(|wake| start.checked_add(wake));
        if wake.is_some_and(|wake| wake <= now) {
            self.scheduler_wake = None;
        }
        scene.into_iter().chain(wake).min()
    }
}
