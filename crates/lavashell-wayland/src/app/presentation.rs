// SPDX-License-Identifier: GPL-3.0-only
use super::{
    Connection, Dispatch, Duration, FrameReport, FrameStage, FrameTime, GlobalList, Instant,
    Platform, PresentedFrame, QueueHandle, sctk,
};
use lavashell_core::frame::{PresentationFlag, PresentationFlags};
use lavashell_core::surface::SurfaceId;
use sctk::reexports::protocols::wp::presentation_time::client::{
    wp_presentation::{self, WpPresentation},
    wp_presentation_feedback::{self, WpPresentationFeedback},
};

fn presentation_flags(kind: wp_presentation_feedback::Kind) -> PresentationFlags {
    use wp_presentation_feedback::Kind;
    [
        (Kind::Vsync, PresentationFlag::Vsync),
        (Kind::HwClock, PresentationFlag::HardwareClock),
        (Kind::HwCompletion, PresentationFlag::HardwareCompletion),
        (Kind::ZeroCopy, PresentationFlag::ZeroCopy),
    ]
    .into_iter()
    .filter(|(wire, _)| kind.contains(*wire))
    .map(|(_, flag)| flag)
    .collect()
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
    pub surface: SurfaceId,
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
        if matches!(event, wp_presentation_feedback::Event::SyncOutput { .. }) {
            return;
        }
        let mut feedback_surface = state
            .surface_index(data.surface)
            .and_then(|index| state.surfaces.get_mut(index));
        if let Some(surface) = feedback_surface.as_deref_mut() {
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
                if let Some(surface) = feedback_surface {
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
                        flags: Some(presentation_flags(
                            wp_presentation_feedback::Kind::from_bits_truncate(flags.into()),
                        )),
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

#[cfg(test)]
mod tests {
    use super::*;
    use wp_presentation_feedback::Kind;

    fn flags_of(kind: Kind) -> [bool; 4] {
        let flags = presentation_flags(kind);
        [
            flags.contains(PresentationFlag::Vsync),
            flags.contains(PresentationFlag::HardwareClock),
            flags.contains(PresentationFlag::HardwareCompletion),
            flags.contains(PresentationFlag::ZeroCopy),
        ]
    }

    #[test]
    fn each_feedback_kind_sets_only_its_own_flag() {
        assert_eq!(flags_of(Kind::Vsync), [true, false, false, false]);
        assert_eq!(flags_of(Kind::HwClock), [false, true, false, false]);
        assert_eq!(flags_of(Kind::HwCompletion), [false, false, true, false]);
        assert_eq!(flags_of(Kind::ZeroCopy), [false, false, false, true]);
        assert_eq!(flags_of(Kind::empty()), [false; 4]);
        assert_eq!(flags_of(Kind::all()), [true; 4]);
    }
}
