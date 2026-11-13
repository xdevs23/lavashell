// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameKind, FrameSequence, Redraw, ViewState};
use lavashell_core::frame::{
    FrameClock, FrameMetrics, FrameTime, PresentationTracker, PresentedFrame,
};
use lavashell_renderer::FrameOutcome;
use std::time::Duration;

/// How far the latest frame of a surface got.
#[derive(Clone, Copy, Debug, Default)]
enum Stage {
    /// The surface can start a frame.
    #[default]
    Ready,
    /// The view composed `frame`, and the scene builder has not built it yet.
    Composing { frame: FrameTime },
    /// The frame with `sequence` renders and has not been reported yet. `display_ready` is set once the display is
    /// ready for the frame that follows.
    Drawing { sequence: u64, display_ready: bool },
    /// A frame was shown, and the display is not ready for the next one yet.
    Shown,
}

/// The pacing state of one surface. Each method that can make the surface start a frame returns the frame started by
/// the surface at `now`.
#[derive(Debug, Default)]
pub(super) struct Pacing {
    clock: FrameClock,
    telemetry: PresentationTracker,
    stage: Stage,
    deadline: Option<Duration>,
    /// The newest state reported by the view since the last frame started.
    wanted: Option<ViewState>,
    next_change: Option<Duration>,
    /// Whether the scene builder built a scene that no frame rendered yet.
    undrawn: bool,
    /// Whether a change of the surface state made outside a compose waits for a commit without a frame.
    uncommitted: bool,
    /// Whether a commit without a frame started and was not reported yet.
    committing: bool,
}

impl Pacing {
    /// Records the next deadline and the state of the view, and starts the frame needed by `view` as `start_wanted`
    /// describes.
    pub fn want(
        &mut self,
        view: ViewState,
        deadline: Option<Duration>,
        redraw: Redraw,
        now: Duration,
    ) -> Option<(FrameKind, FrameTime)> {
        self.deadline = deadline;
        self.wanted = Some(view);
        self.start_wanted(redraw, now)
    }

    pub fn publish(&mut self, next_change: Option<Duration>) {
        self.next_change = next_change;
    }

    /// Records that the scene contains every frame composed up to the frame with `sequence`. Renders the composed
    /// frame once its scene is built. A scene built at any other time, or without a sequence, renders with the next
    /// frame.
    pub fn built(&mut self, sequence: Option<FrameSequence>) -> Option<(FrameKind, FrameTime)> {
        match self.stage {
            Stage::Composing { frame } if sequence >= Some(FrameSequence(frame.sequence)) => {
                Some(self.render(frame))
            }
            Stage::Ready | Stage::Composing { .. } | Stage::Drawing { .. } | Stage::Shown => {
                self.undrawn = true;
                None
            }
        }
    }

    pub fn state_changed(&mut self) {
        self.uncommitted = true;
    }

    pub fn committed(&mut self) {
        self.committing = false;
    }

    /// Starts the commit without a frame of a waiting change of the surface state, while no other commit runs and no
    /// frame of the surface is composed or drawn. Returns whether the commit started.
    pub fn start_commit(&mut self) -> bool {
        let free = matches!(self.stage, Stage::Ready | Stage::Shown);
        let starts = self.uncommitted && free && !self.committing;
        if starts {
            self.uncommitted = false;
            self.committing = true;
        }
        starts
    }

    pub fn output_refresh(&mut self, refresh: Option<Duration>) {
        self.clock.output_refresh(refresh);
    }

    /// Records `outcome` of `frame`. A shown frame sets the render cost to `cost`, and the next frame starts once the
    /// display is ready. If the display became ready while the frame rendered, the frame wanted by the view starts at
    /// once. A stale frame composes again at once, because the view of a frame that was not shown is invalidated. A
    /// pending frame starts again with the next report of the view. A report about another frame changes nothing.
    pub fn rendered(
        &mut self,
        frame: FrameTime,
        outcome: FrameOutcome,
        cost: Duration,
        redraw: Redraw,
        now: Duration,
    ) -> Option<(FrameKind, FrameTime)> {
        let display_ready = match self.stage {
            Stage::Drawing {
                sequence,
                display_ready,
            } if sequence == frame.sequence => display_ready,
            Stage::Ready | Stage::Composing { .. } | Stage::Drawing { .. } | Stage::Shown => {
                return None;
            }
        };
        match outcome {
            FrameOutcome::Shown => {
                self.clock.render_cost(cost);
                self.stage = if display_ready {
                    Stage::Ready
                } else {
                    Stage::Shown
                };
                self.start_wanted(redraw, now)
            }
            FrameOutcome::Stale => Some(self.start(FrameKind::Compose, now)),
            FrameOutcome::Pending => {
                self.stage = Stage::Ready;
                None
            }
        }
    }

    /// Records that the display is ready for the frame that follows a shown frame, also while that frame renders.
    /// Once the surface is ready, starts the frame wanted by the view as `start_wanted` describes.
    pub fn display_ready(
        &mut self,
        redraw: Redraw,
        now: Duration,
    ) -> Option<(FrameKind, FrameTime)> {
        match self.stage {
            Stage::Shown => self.stage = Stage::Ready,
            Stage::Drawing { sequence, .. } => {
                self.stage = Stage::Drawing {
                    sequence,
                    display_ready: true,
                };
            }
            Stage::Ready | Stage::Composing { .. } => {}
        }
        self.start_wanted(redraw, now)
    }

    /// Records that `frame` became visible at `time`, or at a time that cannot be converted to the clock of the app
    /// when `time` is `None`. Returns how evenly the animations advanced since the previous presented frame.
    pub fn presented(
        &mut self,
        frame: FrameTime,
        time: Option<Duration>,
        refresh: Option<Duration>,
    ) -> Option<FrameMetrics> {
        let Some(time) = time else {
            self.clock.output_refresh(refresh);
            self.telemetry.reset();
            return None;
        };
        self.clock
            .presentation_delay(time.saturating_sub(frame.now));
        self.clock.presented(time, refresh);
        self.telemetry.presented(PresentedFrame {
            sequence: frame.sequence,
            animation_time: frame.presentation,
            presentation_time: time,
        })
    }

    /// The next time after `now` at which the surface needs a frame: at the deadline of the view, or early enough
    /// before the next change of the scene for a frame to show that change.
    pub fn wake(&self, now: Duration) -> Option<Duration> {
        let change = self.next_change.map(|change| self.clock.start_for(change));
        [self.deadline, change]
            .into_iter()
            .flatten()
            .filter(|wake| *wake > now)
            .min()
    }

    /// Starts the frame needed by the newest state of the view at `now`, when no other frame of the surface is in
    /// progress and the display is ready. An unchanged view renders the scene when the scene moves or when no frame
    /// rendered the newest built scene.
    fn start_wanted(&mut self, redraw: Redraw, now: Duration) -> Option<(FrameKind, FrameTime)> {
        if !matches!(self.stage, Stage::Ready) {
            return None;
        }
        let kind = match (self.wanted?, redraw) {
            (ViewState::Unconfigured, _) => None,
            (ViewState::Changed, _) | (ViewState::Unchanged, Redraw::Continuous) => {
                Some(FrameKind::Compose)
            }
            (ViewState::Unchanged, Redraw::OnChange) => {
                (self.undrawn || self.moving(now)).then_some(FrameKind::Render)
            }
        };
        kind.map(|kind| self.start(kind, now))
    }

    fn start(&mut self, kind: FrameKind, now: Duration) -> (FrameKind, FrameTime) {
        self.wanted = None;
        let frame = self.clock.sample(now);
        match kind {
            FrameKind::Compose => {
                self.stage = Stage::Composing { frame };
                (kind, frame)
            }
            FrameKind::Render => self.render(frame),
        }
    }

    /// Starts the draw of `frame`. The draw applies the newest built surface state, so a waiting change of the
    /// surface state needs no commit of its own.
    fn render(&mut self, frame: FrameTime) -> (FrameKind, FrameTime) {
        self.stage = Stage::Drawing {
            sequence: frame.sequence,
            display_ready: false,
        };
        self.undrawn = false;
        self.uncommitted = false;
        (FrameKind::Render, frame)
    }

    /// Whether the scene changes by the time at which a frame started at `now` becomes visible.
    fn moving(&self, now: Duration) -> bool {
        let shown = self.clock.predict(now);
        self.next_change.is_some_and(|change| change <= shown)
    }
}
