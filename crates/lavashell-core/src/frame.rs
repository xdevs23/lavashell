// SPDX-License-Identifier: GPL-3.0-only

use crate::numbers::duration_from_nanos;
use std::time::Duration;

mod metrics;
mod report;

pub use metrics::{FrameMetrics, PresentationTracker, PresentedFrame};
pub use report::{
    FrameReport, FrameStage, GpuFrameTime, PresentationFlag, PresentationFlags, RenderStats,
    Submission,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTime {
    pub now: Duration,
    pub presentation: Duration,
    pub delta: Duration,
    pub refresh: Option<Duration>,
    pub sequence: u64,
}

#[derive(Debug, Default)]
pub struct FrameClock {
    refresh: Option<Duration>,
    last_presented: Option<Duration>,
    render_cost: Duration,
    presentation_delay: Duration,
    last_sample: Duration,
    sequence: u64,
}

impl FrameClock {
    pub fn set_output_refresh(&mut self, interval: Option<Duration>) {
        self.refresh = interval.filter(|interval| !interval.is_zero());
    }

    pub fn record_presentation(&mut self, time: Duration, refresh: Option<Duration>) {
        if self.last_presented.is_some_and(|last| time <= last) {
            return;
        }
        self.set_output_refresh(refresh);
        self.last_presented = Some(time);
    }

    pub fn set_render_cost(&mut self, cost: Duration) {
        self.render_cost = cost;
    }

    pub fn set_presentation_delay(&mut self, delay: Duration) {
        self.presentation_delay = delay;
    }

    pub fn sample(&mut self, now: Duration) -> FrameTime {
        let target = self.predict(now);
        let delta = target.saturating_sub(self.last_sample);
        self.last_sample = target;
        self.sequence += 1;
        FrameTime {
            now,
            presentation: target,
            delta,
            refresh: self.refresh,
            sequence: self.sequence,
        }
    }

    #[must_use]
    pub fn predict(&self, now: Duration) -> Duration {
        let target = match (self.last_presented, self.refresh) {
            (Some(last_presented), Some(interval)) => {
                let rendered = now + self.render_cost;
                let elapsed_refreshes =
                    rendered.saturating_sub(last_presented).as_nanos() / interval.as_nanos();
                let next_refresh =
                    last_presented.as_nanos() + interval.as_nanos() * (elapsed_refreshes + 1);
                duration_from_nanos(next_refresh)
            }
            _ => now + self.presentation_delay,
        };
        target.max(self.last_sample)
    }

    #[must_use]
    pub fn start_for(&self, presentation: Duration) -> Duration {
        if self.last_sample >= presentation {
            return Duration::ZERO;
        }
        match (self.last_presented, self.refresh) {
            (Some(last_presented), Some(interval)) => {
                let since_presented = presentation.saturating_sub(last_presented).as_nanos();
                let refreshes = since_presented
                    .div_ceil(interval.as_nanos())
                    .saturating_sub(1);
                if refreshes == 0 {
                    return Duration::ZERO;
                }
                let refresh_start = last_presented.as_nanos() + interval.as_nanos() * refreshes;
                duration_from_nanos(refresh_start).saturating_sub(self.render_cost)
            }
            _ => presentation.saturating_sub(self.presentation_delay),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_refresh_samples_regular_deadlines_despite_callback_jitter() {
        let mut clock = FrameClock::default();
        let period = Duration::from_nanos(8_333_333);
        for i in 1..500 {
            let phase = period * i;
            clock.record_presentation(phase, Some(period));
            let jitter = Duration::from_micros(if i % 2 == 0 { 100 } else { 1100 });
            let frame = clock.sample(phase + jitter);
            assert_eq!(frame.presentation, phase + period);
            if i > 1 {
                assert_eq!(frame.delta, period);
            }
        }
    }

    #[test]
    fn fixed_refresh_targets_the_first_refresh_after_the_render_cost() {
        let mut clock = FrameClock::default();
        let period = Duration::from_millis(10);
        clock.record_presentation(Duration::from_millis(100), Some(period));
        clock.set_render_cost(Duration::from_millis(4));
        assert_eq!(
            clock.predict(Duration::from_millis(101)),
            Duration::from_millis(110)
        );
        assert_eq!(
            clock.predict(Duration::from_millis(107)),
            Duration::from_millis(120)
        );
        clock.set_render_cost(Duration::from_millis(23));
        assert_eq!(
            clock.predict(Duration::from_millis(101)),
            Duration::from_millis(130)
        );
    }

    #[test]
    fn fixed_refresh_starts_a_frame_for_a_presentation_at_the_first_start_that_reaches_it() {
        let mut clock = FrameClock::default();
        clock.record_presentation(Duration::from_millis(16), Some(Duration::from_millis(10)));
        clock.set_render_cost(Duration::from_millis(1));
        let start = clock.start_for(Duration::from_millis(150));
        assert_eq!(start, Duration::from_millis(145));
        assert_eq!(clock.predict(start), Duration::from_millis(156));
        assert_eq!(
            clock.predict(start.saturating_sub(Duration::from_nanos(1))),
            Duration::from_millis(146)
        );
        assert_eq!(clock.start_for(Duration::from_millis(26)), Duration::ZERO);
    }

    #[test]
    fn variable_refresh_targets_the_last_measured_delay() {
        let mut clock = FrameClock::default();
        clock.record_presentation(Duration::from_millis(50), None);
        clock.set_presentation_delay(Duration::from_millis(9));
        clock.set_presentation_delay(Duration::from_millis(14));
        let frame = clock.sample(Duration::from_millis(61));
        assert_eq!(frame.presentation, Duration::from_millis(75));
        assert_eq!(frame.refresh, None);
    }

    #[test]
    fn fixed_refresh_without_a_presentation_targets_the_last_measured_delay() {
        let mut clock = FrameClock::default();
        clock.set_output_refresh(Some(Duration::from_millis(10)));
        clock.set_presentation_delay(Duration::from_millis(12));
        let frame = clock.sample(Duration::from_millis(3));
        assert_eq!(frame.presentation, Duration::from_millis(15));
        assert_eq!(frame.refresh, Some(Duration::from_millis(10)));
    }

    #[test]
    fn the_refresh_reported_with_a_presentation_replaces_the_previous_one() {
        let mut clock = FrameClock::default();
        let period = Duration::from_nanos(6_944_444);
        clock.record_presentation(Duration::from_millis(8), None);
        clock.record_presentation(Duration::from_millis(60), Some(period));
        assert_eq!(
            clock.sample(Duration::from_millis(61)).refresh,
            Some(period)
        );
        clock.record_presentation(Duration::from_millis(70), Some(Duration::ZERO));
        assert_eq!(clock.sample(Duration::from_millis(71)).refresh, None);
    }

    #[test]
    fn a_prediction_is_the_presentation_of_the_next_sample_and_changes_nothing() {
        let mut clock = FrameClock::default();
        let period = Duration::from_nanos(8_333_333);
        clock.record_presentation(period, Some(period));
        let now = period + Duration::from_millis(1);
        let predicted = clock.predict(now);
        assert_eq!(clock.predict(now), predicted);
        let frame = clock.sample(now);
        assert_eq!(frame.presentation, predicted);
        assert_eq!(frame.sequence, 1);
    }

    #[test]
    fn old_feedback_and_backwards_predictions_never_reverse_time() {
        let mut clock = FrameClock::default();
        assert_eq!(
            clock.sample(Duration::from_secs(1)).presentation,
            Duration::from_secs(1)
        );
        clock.record_presentation(Duration::from_millis(100), None);
        clock.record_presentation(Duration::from_millis(80), Some(Duration::from_millis(10)));
        assert_eq!(clock.sample(Duration::from_millis(500)).refresh, None);
        assert_eq!(
            clock.sample(Duration::from_millis(500)).presentation,
            Duration::from_secs(1)
        );
    }
}
