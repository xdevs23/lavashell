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

/// The kind of frame started by a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// The view composes the frame. The frame renders once the scene builder built it.
    Compose,
    /// Only motions of the scene change, so the scene is rendered without composing the view.
    Render,
}

/// The sequence number of a frame started by the frame scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameSequence(pub u64);

/// What the view of a surface needs for the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewState {
    /// The host has not configured the surface yet, so the surface draws nothing.
    Unconfigured,
    /// The view looks the same as in the last frame.
    Unchanged,
    /// The view changed or has actions queued.
    Changed,
}

/// When the views of an app compose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Redraw {
    /// Every frame composes the view, because the app draws on every frame.
    Continuous,
    /// A view composes only when it changed.
    OnChange,
}

/// What the frame scheduler learns about a surface.
#[derive(Clone, Copy, Debug)]
pub enum SchedulerInput {
    /// The surface opened. The scheduler ignores every other input about a surface that is not open.
    Opened(SurfaceId),
    Timing(TimingEvent),
    /// What the view of the surface needs, and the next timed change of the view at `deadline`. Each report replaces
    /// the previous one.
    Wants {
        surface: SurfaceId,
        view: ViewState,
        deadline: Option<Duration>,
    },
    /// The scene of the surface changes by itself at `next_change`, or only with the next frame of the view when it
    /// is `None`. Each report replaces the previous one.
    Published {
        surface: SurfaceId,
        next_change: Option<Duration>,
    },
    /// The scene of the surface contains every frame that the view composed up to the frame with `sequence`. A
    /// scene built from a frame sent outside a compose has no sequence.
    Built {
        surface: SurfaceId,
        sequence: Option<FrameSequence>,
    },
    /// The surface state changed outside a compose. The scheduler starts a commit without a frame for the change
    /// once no frame of the surface is composed or drawn. A frame drawn before that applies the change.
    StateChanged(SurfaceId),
    /// The host received the commit started by [`SchedulerOutput::CommitState`].
    Committed(SurfaceId),
    /// The frame for `frame` was drawn with `outcome`, which took `cost`.
    Rendered {
        surface: SurfaceId,
        frame: FrameTime,
        outcome: FrameOutcome,
        cost: Duration,
    },
    /// The smallest refresh interval among the outputs that show the surface, or `None` when none of them reports
    /// one. A platform sends it only when the host reports no presentation times.
    OutputRefresh {
        surface: SurfaceId,
        refresh: Option<Duration>,
    },
    /// The surface closed. Its pacing state is dropped.
    Closed(SurfaceId),
}

/// What the frame scheduler asks for.
#[derive(Clone, Debug)]
pub enum SchedulerOutput {
    /// Start a frame of `kind` for the surface at `time`.
    Frame {
        surface: SurfaceId,
        kind: FrameKind,
        time: FrameTime,
    },
    /// Commit the newest built surface state of the surface without a frame, and report it with
    /// [`SchedulerInput::Committed`].
    CommitState(SurfaceId),
    /// Pass the report to the frame observer of the app.
    Report(Box<FrameReport>),
    /// Ask every view at this time what it needs. A surface can need a frame then without another input. `None`
    /// cancels the earlier time. Each output replaces the previous one.
    WakeAt(Option<Duration>),
}

/// Decides when each surface composes and renders. A surface starts a frame when the view of the surface needs one or
/// the scene moves. No earlier frame of the surface may be in progress or waiting for the display. A composed frame
/// renders once the scene builder built it. The frame clock of each surface predicts when the frame becomes visible
/// from the reports of the host and the render cost.
#[derive(Debug)]
pub struct FrameScheduler {
    redraw: Redraw,
    surfaces: BTreeMap<SurfaceId, Pacing>,
    /// The wake time sent last.
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

    /// Applies `input` at `now` and appends the frames, commits and reports that follow from it to `out`, followed by
    /// the wake time when the input changed it. A report about a surface unknown to the scheduler leaves the pacing
    /// state unchanged.
    pub fn handle(&mut self, now: Duration, input: SchedulerInput, out: &mut Vec<SchedulerOutput>) {
        let surface = input.surface();
        let started = match input {
            SchedulerInput::Opened(surface) => {
                let _reopened = self.surfaces.insert(surface, Pacing::default());
                None
            }
            SchedulerInput::Timing(event) => {
                self.handle_timing(now, event, out);
                None
            }
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
            } => (self.surfaces.get_mut(&surface))
                .and_then(|pacing| pacing.rendered(frame, outcome, cost, now))
                .map(|frame| (surface, frame)),
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

    /// The earliest time after `now` at which a surface needs a frame without another input.
    fn next_wake(&self, now: Duration) -> Option<Duration> {
        (self.surfaces.values())
            .filter_map(|pacing| pacing.wake(now))
            .min()
    }

    fn handle_timing(&mut self, now: Duration, event: TimingEvent, out: &mut Vec<SchedulerOutput>) {
        match event {
            TimingEvent::DisplayReady(surface) => {
                if let Some(pacing) = self.surfaces.get_mut(&surface) {
                    pacing.display_ready();
                }
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
    }
}

impl SchedulerInput {
    /// The surface named by the input, or `None` for a timing event.
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
