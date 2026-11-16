// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::frame::{FrameReport, FrameStage, FrameTime};
use lavashell_core::surface::SurfaceId;
use lavashell_platform::TimingEvent;
use lavashell_renderer::FrameOutcome;
use pacing::Pacing;
use std::collections::BTreeMap;
use std::mem;
use std::time::Duration;

mod pacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Compose,
    Render,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameSequence(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewState {
    Unconfigured,
    Unchanged,
    Changed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redraw {
    Continuous,
    OnChange,
}

#[derive(Clone, Copy, Debug)]
pub enum SchedulerInput {
    Opened(SurfaceId),
    Timing(TimingEvent),
    Wants {
        surface: SurfaceId,
        view: ViewState,
        deadline: Option<Duration>,
    },
    Published {
        surface: SurfaceId,
        next_change: Option<Duration>,
    },
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
    },
    StateChanged(SurfaceId),
    Committed(SurfaceId),
    Rendered {
        surface: SurfaceId,
        frame: FrameTime,
        outcome: FrameOutcome,
        cost: Duration,
    },
    OutputRefresh {
        surface: SurfaceId,
        refresh: Option<Duration>,
    },
    Closed(SurfaceId),
}

#[derive(Clone, Debug)]
pub enum SchedulerOutput {
    Frame {
        surface: SurfaceId,
        kind: FrameKind,
        time: FrameTime,
    },
    CommitState(SurfaceId),
    Report(Box<FrameReport>),
    WakeAt(Option<Duration>),
}

#[derive(Debug)]
pub(crate) struct FrameScheduler {
    redraw: Redraw,
    surfaces: BTreeMap<SurfaceId, Pacing>,
    announced_wake: Option<Duration>,
}

impl FrameScheduler {
    #[must_use]
    pub const fn new(redraw: Redraw) -> Self {
        Self {
            redraw,
            surfaces: BTreeMap::new(),
            announced_wake: None,
        }
    }

    pub fn handle(&mut self, now: Duration, input: SchedulerInput, out: &mut Vec<SchedulerOutput>) {
        let surface = input.surface();
        let started = match input {
            SchedulerInput::Opened(surface) => {
                let _pacing_of_a_reopened_surface_starts_again =
                    self.surfaces.insert(surface, Pacing::default());
                None
            }
            SchedulerInput::Timing(event) => self.handle_timing(now, event, out),
            SchedulerInput::Wants {
                surface,
                view,
                deadline,
            } => {
                let redraw = self.redraw;
                (self.surfaces.get_mut(&surface))
                    .and_then(|pacing| pacing.want(view, deadline, redraw, now))
                    .map(|frame| (surface, frame))
            }
            SchedulerInput::Published {
                surface,
                next_change,
            } => {
                if let Some(pacing) = self.surfaces.get_mut(&surface) {
                    pacing.publish(next_change);
                }
                None
            }
            SchedulerInput::Built { surface, sequence } => (self.surfaces.get_mut(&surface))
                .and_then(|pacing| pacing.built(sequence))
                .map(|frame| (surface, frame)),
            SchedulerInput::StateChanged(surface) => {
                if let Some(pacing) = self.surfaces.get_mut(&surface) {
                    pacing.state_changed();
                }
                None
            }
            SchedulerInput::Committed(surface) => {
                if let Some(pacing) = self.surfaces.get_mut(&surface) {
                    pacing.committed();
                }
                None
            }
            SchedulerInput::Rendered {
                surface,
                frame,
                outcome,
                cost,
            } => {
                let redraw = self.redraw;
                (self.surfaces.get_mut(&surface))
                    .and_then(|pacing| pacing.rendered(frame, outcome, cost, redraw, now))
                    .map(|frame| (surface, frame))
            }
            SchedulerInput::OutputRefresh { surface, refresh } => {
                if let Some(pacing) = self.surfaces.get_mut(&surface) {
                    pacing.output_refresh(refresh);
                }
                None
            }
            SchedulerInput::Closed(surface) => {
                let _dropped_pacing = self.surfaces.remove(&surface);
                None
            }
        };
        if let Some((surface, (kind, time))) = started {
            out.push(SchedulerOutput::Frame {
                surface,
                kind,
                time,
            });
        }
        if let Some(surface) = surface
            && (self.surfaces.get_mut(&surface)).is_some_and(Pacing::start_commit)
        {
            out.push(SchedulerOutput::CommitState(surface));
        }
        let wake = self.next_wake(now);
        if mem::replace(&mut self.announced_wake, wake) != wake {
            out.push(SchedulerOutput::WakeAt(wake));
        }
    }

    fn next_wake(&self, now: Duration) -> Option<Duration> {
        (self.surfaces.values())
            .filter_map(|pacing| pacing.wake(now))
            .min()
    }

    fn handle_timing(
        &mut self,
        now: Duration,
        event: TimingEvent,
        out: &mut Vec<SchedulerOutput>,
    ) -> Option<(SurfaceId, (FrameKind, FrameTime))> {
        match event {
            TimingEvent::DisplayReady(surface) => {
                let redraw = self.redraw;
                return (self.surfaces.get_mut(&surface))
                    .and_then(|pacing| pacing.display_ready(redraw, now))
                    .map(|frame| (surface, frame));
            }
            TimingEvent::Presented {
                surface,
                frame,
                time,
                refresh,
                flags,
            } => {
                let metrics = (self.surfaces.get_mut(&surface))
                    .and_then(|pacing| pacing.presented(frame, time, refresh));
                out.push(SchedulerOutput::Report(Box::new(FrameReport {
                    surface,
                    timing: frame,
                    stage: FrameStage::Presented,
                    observed: time.unwrap_or(now),
                    presentation_time: time,
                    metrics,
                    flags: Some(flags),
                    submission: None,
                })));
            }
            TimingEvent::Discarded { surface, frame } => {
                out.push(SchedulerOutput::Report(Box::new(FrameReport {
                    surface,
                    timing: frame,
                    stage: FrameStage::Discarded,
                    observed: now,
                    presentation_time: None,
                    metrics: None,
                    flags: None,
                    submission: None,
                })));
            }
        }
        None
    }
}

impl SchedulerInput {
    fn surface(&self) -> Option<SurfaceId> {
        match *self {
            Self::Opened(surface)
            | Self::Wants { surface, .. }
            | Self::Published { surface, .. }
            | Self::Built { surface, .. }
            | Self::StateChanged(surface)
            | Self::Committed(surface)
            | Self::Rendered { surface, .. }
            | Self::OutputRefresh { surface, .. }
            | Self::Closed(surface) => Some(surface),
            Self::Timing(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
