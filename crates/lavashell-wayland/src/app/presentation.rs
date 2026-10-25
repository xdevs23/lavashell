// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Connection, Dispatch, Duration, FrameMetrics, FrameTime, GlobalList, Instant, Platform,
    PresentedFrame, QueueHandle, sctk,
};
use lavashell_core::view::ViewStats;
use lavashell_renderer::{GpuFrameTime, RenderStats};
use sctk::reexports::protocols::wp::presentation_time::client::{
    wp_presentation::{self, WpPresentation},
    wp_presentation_feedback::{self, WpPresentationFeedback},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum FrameStage {
    Submitted,
    Presented,
    Discarded,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameReport {
    pub surface: usize,
    pub timing: FrameTime,
    pub stage: FrameStage,
    /// The time of the presentation, measured from the start of the app. For a discarded frame, or when the clock
    /// of the compositor cannot be converted to the clock of the app, the time at which the feedback arrived.
    pub observed: Duration,
    /// `None` when only the arrival time of the feedback is known. A report without this time cannot be used to
    /// measure display timing or the error of an animation.
    pub presentation_time: Option<Duration>,
    pub metrics: Option<FrameMetrics>,
    pub flags: Option<PresentationFlags>,
    pub submission: Option<Submission>,
}

#[derive(Clone, Copy, Debug)]
pub struct Submission {
    /// The wall-clock time spent on composition, painting and submission, including time in which the thread was
    /// preempted.
    pub render_cost: Duration,
    /// The CPU time of the calling thread within the same span, without the time of other threads or of the GPU.
    pub cpu_time: Duration,
    pub view: ViewStats,
    pub render: RenderStats,
    /// The generation of the scene image drawn beneath the view.
    pub scene: Option<u64>,
    /// When GPU timing is enabled on the app, the GPU time of the most recent earlier frame of the surface that was
    /// measured since the last report.
    pub gpu: Option<GpuFrameTime>,
}

/// How the compositor presented a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationFlags(wp_presentation_feedback::Kind);

impl PresentationFlags {
    /// The presentation was synchronized to the vertical retrace of the display.
    #[must_use]
    pub fn vsync(self) -> bool {
        self.0.contains(wp_presentation_feedback::Kind::Vsync)
    }

    /// The timestamp comes from the clock of the display hardware.
    #[must_use]
    pub fn hardware_clock(self) -> bool {
        self.0.contains(wp_presentation_feedback::Kind::HwClock)
    }

    /// The display hardware signalled that the presentation completed.
    #[must_use]
    pub fn hardware_completion(self) -> bool {
        self.0
            .contains(wp_presentation_feedback::Kind::HwCompletion)
    }

    /// The display scanned the buffer out without a copy.
    #[must_use]
    pub fn zero_copy(self) -> bool {
        self.0.contains(wp_presentation_feedback::Kind::ZeroCopy)
    }
}

impl From<wp_presentation_feedback::Kind> for PresentationFlags {
    fn from(kind: wp_presentation_feedback::Kind) -> Self {
        Self(kind)
    }
}

pub(super) struct Presentation {
    pub proxy: WpPresentation,
    epoch: Option<Duration>,
    clock: Option<rustix::time::ClockId>,
}

impl Presentation {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Platform>) -> Option<Self> {
        Some(Self {
            proxy: globals.bind(qh, 1..=2, ()).ok()?,
            epoch: None,
            clock: None,
        })
    }

    fn observed(&self, absolute: Duration, start: Instant) -> Option<Duration> {
        if self.clock == Some(rustix::time::ClockId::Monotonic) {
            return absolute.checked_sub(self.epoch?);
        }
        // Other clocks can drift or be adjusted against the monotonic clock, or count time in suspend. The
        // timestamp is converted with the current value of its clock, because an offset measured at startup would
        // be out of date.
        let before = start.elapsed();
        let clock_now = Duration::try_from(rustix::time::clock_gettime(self.clock?)).ok()?;
        let after = start.elapsed();
        let age = clock_now.checked_sub(absolute)?;
        let read_at = midpoint(before, after);
        read_at.checked_sub(age)
    }
}

/// The best estimate of when something happened between `earlier` and `later`.
fn midpoint(earlier: Duration, later: Duration) -> Duration {
    earlier + later.saturating_sub(earlier) / 2
}

pub(super) struct Feedback {
    pub surface: usize,
    pub timing: FrameTime,
}

impl Dispatch<WpPresentation, ()> for Platform {
    fn event(
        state: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            // Only a clock that this process can read is used. With an unknown clock, `observed` returns `None`,
            // and the frame clock continues to time frames by frame callbacks, so that timestamps of different
            // clocks are not mixed.
            let clock = i32::try_from(clk_id)
                .ok()
                .and_then(|id| rustix::time::ClockId::try_from(id).ok());
            if let Some(clock) = clock
                && let Ok(absolute) = Duration::try_from(rustix::time::clock_gettime(clock))
                && let Some(presentation) = &mut state.presentation
            {
                presentation.epoch = absolute.checked_sub(state.start.elapsed());
                presentation.clock = Some(clock);
            }
        }
    }
}

impl Dispatch<WpPresentationFeedback, Feedback> for Platform {
    fn event(
        state: &mut Self,
        proxy: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &Feedback,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if !matches!(event, wp_presentation_feedback::Event::SyncOutput { .. })
            && let Some(surface) = state.surfaces.get_mut(data.surface)
        {
            surface.feedback.retain(|feedback| feedback != proxy);
        }
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                flags,
                ..
            } => {
                let absolute =
                    Duration::new((u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo), tv_nsec);
                let observed = state
                    .presentation
                    .as_ref()
                    .and_then(|presentation| presentation.observed(absolute, state.start));
                let interval = (refresh != 0).then(|| Duration::from_nanos(u64::from(refresh)));
                let mut metrics = None;
                if let Some(surface) = state.surfaces.get_mut(data.surface) {
                    if let Some(time) = observed {
                        surface
                            .clock
                            .presentation_delay(time.saturating_sub(data.timing.now));
                        surface.clock.presented(time, interval);
                        if state.config.observer.is_some() {
                            metrics = surface.telemetry.presented(PresentedFrame {
                                sequence: data.timing.sequence,
                                animation_time: data.timing.presentation,
                                presentation_time: time,
                            });
                        }
                    } else {
                        surface.clock.output_refresh(interval);
                        surface.telemetry.reset();
                    }
                }
                if let Some(observer) = &mut state.config.observer {
                    observer(FrameReport {
                        surface: data.surface,
                        timing: data.timing,
                        stage: FrameStage::Presented,
                        observed: observed.unwrap_or_else(|| state.start.elapsed()),
                        presentation_time: observed,
                        metrics,
                        flags: Some(
                            wp_presentation_feedback::Kind::from_bits_truncate(flags.into()).into(),
                        ),
                        submission: None,
                    });
                }
            }
            wp_presentation_feedback::Event::Discarded => {
                if let Some(observer) = &mut state.config.observer {
                    observer(FrameReport {
                        surface: data.surface,
                        timing: data.timing,
                        stage: FrameStage::Discarded,
                        observed: state.start.elapsed(),
                        presentation_time: None,
                        metrics: None,
                        flags: None,
                        submission: None,
                    });
                }
            }
            _ => {}
        }
    }
}
