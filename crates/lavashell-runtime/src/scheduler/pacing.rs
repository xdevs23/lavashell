// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameKind, FrameSequence, Redraw, ViewState};
use lavashell_core::frame::{
    FrameClock, FrameMetrics, FrameTime, PresentationTracker, PresentedFrame,
};
use lavashell_vulkan_renderer::FrameOutcome;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
enum Stage {
    #[default]
    Ready,
    Composing {
        frame: FrameTime,
    },
    Drawing {
        sequence: u64,
        display_ready: bool,
    },
    Shown,
}

#[derive(Debug, Default)]
pub(super) struct Pacing {
    clock: FrameClock,
    telemetry: PresentationTracker,
    stage: Stage,
    deadline: Option<Duration>,
    wanted: Option<ViewState>,
    next_change: Option<Duration>,
    undrawn: bool,
    uncommitted: bool,
    committing: bool,
}

impl Pacing {
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
        self.clock.set_output_refresh(refresh);
    }

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
                self.clock.set_render_cost(cost);
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

    pub fn presented(
        &mut self,
        frame: FrameTime,
        time: Option<Duration>,
        refresh: Option<Duration>,
    ) -> Option<FrameMetrics> {
        let Some(time) = time else {
            self.clock.set_output_refresh(refresh);
            self.telemetry.reset();
            return None;
        };
        self.clock
            .set_presentation_delay(time.saturating_sub(frame.now));
        self.clock.record_presentation(time, refresh);
        self.telemetry.presented(PresentedFrame {
            sequence: frame.sequence,
            animation_time: frame.presentation,
            presentation_time: time,
        })
    }

    pub fn wake(&self, now: Duration) -> Option<Duration> {
        let change = self.next_change.map(|change| self.clock.start_for(change));
        [self.deadline, change]
            .into_iter()
            .flatten()
            .filter(|wake| *wake > now)
            .min()
    }

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

    fn render(&mut self, frame: FrameTime) -> (FrameKind, FrameTime) {
        self.stage = Stage::Drawing {
            sequence: frame.sequence,
            display_ready: false,
        };
        self.undrawn = false;
        self.uncommitted = false;
        (FrameKind::Render, frame)
    }

    fn moving(&self, now: Duration) -> bool {
        let shown = self.clock.predict(now);
        self.next_change.is_some_and(|change| change <= shown)
    }
}
