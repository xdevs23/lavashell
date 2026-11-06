// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameKind, Redraw, ViewState};
use lavashell_core::frame::{
    FrameClock, FrameMetrics, FrameTime, PresentationTracker, PresentedFrame,
};
use lavashell_renderer::FrameOutcome;
use std::time::Duration;

/// How far the latest frame of a surface got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    /// The surface can start a frame.
    #[default]
    Ready,
    /// The frame with `sequence` started and has not been rendered yet.
    Drawing { sequence: u64 },
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
    next_change: Option<Duration>,
}

impl Pacing {
    /// Records the next deadline of the view. Starts the frame needed by `view` at `now`, when no other frame of the
    /// surface is in progress or waits for the display.
    pub fn want(
        &mut self,
        view: ViewState,
        deadline: Option<Duration>,
        redraw: Redraw,
        now: Duration,
    ) -> Option<(FrameKind, FrameTime)> {
        self.deadline = deadline;
        if self.stage != Stage::Ready {
            return None;
        }
        let kind = match (view, redraw) {
            (ViewState::Unconfigured, _) => None,
            (ViewState::Changed, _) | (ViewState::Unchanged, Redraw::Continuous) => {
                Some(FrameKind::Compose)
            }
            (ViewState::Unchanged, Redraw::OnChange) => {
                self.moving(now).then_some(FrameKind::Render)
            }
        };
        kind.map(|kind| self.start(kind, now))
    }

    pub fn publish(&mut self, next_change: Option<Duration>) {
        self.next_change = next_change;
    }

    pub fn output_refresh(&mut self, refresh: Option<Duration>) {
        self.clock.output_refresh(refresh);
    }

    /// Records `outcome` of `frame`. A shown frame sets the render cost to `cost`, and waits for the display. A stale
    /// frame composes again at once, because the view of a frame that was not shown is invalidated. A pending frame
    /// starts again with the next report of the view. A report about another frame changes nothing.
    pub fn rendered(
        &mut self,
        frame: FrameTime,
        outcome: FrameOutcome,
        cost: Duration,
        now: Duration,
    ) -> Option<(FrameKind, FrameTime)> {
        let drawing = Stage::Drawing {
            sequence: frame.sequence,
        };
        if self.stage != drawing {
            return None;
        }
        match outcome {
            FrameOutcome::Shown => {
                self.clock.render_cost(cost);
                self.stage = Stage::Shown;
                None
            }
            FrameOutcome::Stale => Some(self.start(FrameKind::Compose, now)),
            FrameOutcome::Pending => {
                self.stage = Stage::Ready;
                None
            }
        }
    }

    /// Records that the display is ready for the frame that follows a shown frame. That frame starts with the next
    /// report of the view.
    pub fn display_ready(&mut self) {
        if self.stage == Stage::Shown {
            self.stage = Stage::Ready;
        }
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
        let lead = self.clock.predict(now).saturating_sub(now);
        let change = self.next_change.map(|change| change.saturating_sub(lead));
        [self.deadline, change]
            .into_iter()
            .flatten()
            .filter(|wake| *wake > now)
            .min()
    }

    fn start(&mut self, kind: FrameKind, now: Duration) -> (FrameKind, FrameTime) {
        let time = self.clock.sample(now);
        self.stage = Stage::Drawing {
            sequence: time.sequence,
        };
        (kind, time)
    }

    /// Whether the scene changes by the time at which a frame started at `now` becomes visible.
    fn moving(&self, now: Duration) -> bool {
        let shown = self.clock.predict(now);
        self.next_change.is_some_and(|change| change <= shown)
    }
}
