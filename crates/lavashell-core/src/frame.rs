// SPDX-License-Identifier: GPL-3.0-only
//! The prediction of the time at which a frame becomes visible, made from what the host measured about earlier
//! frames. Animations sample their values at that time. Every `Duration` here counts from one monotonic start time
//! per application.

use crate::numbers::duration_from_nanos;
use std::time::Duration;

mod metrics;

pub use metrics::{FrameMetrics, PresentationTracker, PresentedFrame};

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTime {
    /// The monotonic time at which the host sampled the clock for this frame.
    pub now: Duration,
    /// The predicted time at which this frame becomes visible. Animations sample their values at this time.
    pub presentation: Duration,
    /// The time from the presentation time of the previous frame to the presentation time of this frame.
    pub delta: Duration,
    /// Refresh interval reported by the host, or None for an unknown or variable refresh interval.
    pub refresh: Option<Duration>,
    pub sequence: u64,
}

/// Predicts when a frame becomes visible from the measurements reported by the host about earlier frames. The host
/// chooses when to render a frame.
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
    /// Sets the refresh interval of the output. None or zero marks an unknown or variable refresh.
    pub fn output_refresh(&mut self, interval: Option<Duration>) {
        self.refresh = interval.filter(|interval| !interval.is_zero());
    }

    /// Records the time a frame became visible and the refresh interval reported with it. A time that is not later
    /// than the last recorded presentation is ignored.
    pub fn presented(&mut self, time: Duration, refresh: Option<Duration>) {
        if self.last_presented.is_some_and(|last| time <= last) {
            return;
        }
        self.output_refresh(refresh);
        self.last_presented = Some(time);
    }

    /// Records how long the last frame took to render.
    pub fn render_cost(&mut self, cost: Duration) {
        self.render_cost = cost;
    }

    /// Records the measured time from the start of the last presented frame until it became visible.
    pub fn presentation_delay(&mut self, delay: Duration) {
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

    /// The time at which a frame started at `now` becomes visible. With a fixed refresh interval and a recorded
    /// presentation, this is the first refresh after `now` plus the render cost. Otherwise it is `now` plus the last
    /// measured presentation delay. The prediction is never earlier than the presentation time of the last sample.
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
            clock.presented(phase, Some(period));
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
        clock.presented(Duration::from_millis(100), Some(period));
        clock.render_cost(Duration::from_millis(4));
        assert_eq!(
            clock.predict(Duration::from_millis(101)),
            Duration::from_millis(110)
        );
        assert_eq!(
            clock.predict(Duration::from_millis(107)),
            Duration::from_millis(120)
        );
        clock.render_cost(Duration::from_millis(23));
        assert_eq!(
            clock.predict(Duration::from_millis(101)),
            Duration::from_millis(130)
        );
    }

    #[test]
    fn variable_refresh_targets_the_last_measured_delay() {
        let mut clock = FrameClock::default();
        clock.presented(Duration::from_millis(50), None);
        clock.presentation_delay(Duration::from_millis(9));
        clock.presentation_delay(Duration::from_millis(14));
        let frame = clock.sample(Duration::from_millis(61));
        assert_eq!(frame.presentation, Duration::from_millis(75));
        assert_eq!(frame.refresh, None);
    }

    #[test]
    fn fixed_refresh_without_a_presentation_targets_the_last_measured_delay() {
        let mut clock = FrameClock::default();
        clock.output_refresh(Some(Duration::from_millis(10)));
        clock.presentation_delay(Duration::from_millis(12));
        let frame = clock.sample(Duration::from_millis(3));
        assert_eq!(frame.presentation, Duration::from_millis(15));
        assert_eq!(frame.refresh, Some(Duration::from_millis(10)));
    }

    #[test]
    fn the_refresh_reported_with_a_presentation_replaces_the_previous_one() {
        let mut clock = FrameClock::default();
        let period = Duration::from_nanos(6_944_444);
        clock.presented(Duration::from_millis(8), None);
        clock.presented(Duration::from_millis(60), Some(period));
        assert_eq!(
            clock.sample(Duration::from_millis(61)).refresh,
            Some(period)
        );
        clock.presented(Duration::from_millis(70), Some(Duration::ZERO));
        assert_eq!(clock.sample(Duration::from_millis(71)).refresh, None);
    }

    #[test]
    fn a_prediction_is_the_presentation_of_the_next_sample_and_changes_nothing() {
        let mut clock = FrameClock::default();
        let period = Duration::from_nanos(8_333_333);
        clock.presented(period, Some(period));
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
        clock.presented(Duration::from_millis(100), None);
        clock.presented(Duration::from_millis(80), Some(Duration::from_millis(10)));
        assert_eq!(clock.sample(Duration::from_millis(500)).refresh, None);
        assert_eq!(
            clock.sample(Duration::from_millis(500)).presentation,
            Duration::from_secs(1)
        );
    }
}
