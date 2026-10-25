// SPDX-License-Identifier: GPL-3.0-only
//! Measurements of how evenly animations advance between two consecutive frames that became visible on a surface.

use crate::numbers::NANOS_PER_SECOND;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct PresentedFrame {
    pub sequence: u64,
    /// The time at which the animation host sampled its animations for this frame.
    pub animation_time: Duration,
    /// The time at which the compositor reports that the frame became visible, converted to the clock of
    /// `animation_time`.
    pub presentation_time: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMetrics {
    pub animation_step: Duration,
    pub display_step: Duration,
    /// `animation_step` minus `display_step` in nanoseconds. A difference of either sign makes motion stutter.
    pub animation_error_ns: i128,
    /// `animation_time` minus `presentation_time` of the newer frame in nanoseconds. A constant offset delays every
    /// frame by the same time and does not make motion stutter.
    pub prediction_error_ns: i128,
}

/// Compares each frame presented on one surface with the frame presented before it. Every surface that presents
/// independently needs a separate tracker. A frame that was submitted but never displayed must not be passed to the
/// tracker. Two displayed frames that the tracker compares can have discarded frames between them.
#[derive(Debug, Default)]
pub struct PresentationTracker {
    last: Option<PresentedFrame>,
}

impl PresentationTracker {
    pub fn presented(&mut self, frame: PresentedFrame) -> Option<FrameMetrics> {
        let Some(last) = self.last else {
            self.last = Some(frame);
            return None;
        };
        if frame.sequence <= last.sequence {
            return None;
        }
        let animation_step = frame.animation_time.checked_sub(last.animation_time)?;
        let display_step = frame
            .presentation_time
            .checked_sub(last.presentation_time)
            .filter(|step| !step.is_zero())?;
        self.last = Some(frame);
        Some(FrameMetrics {
            animation_step,
            display_step,
            animation_error_ns: signed_nanos(animation_step) - signed_nanos(display_step),
            prediction_error_ns: signed_nanos(frame.animation_time)
                - signed_nanos(frame.presentation_time),
        })
    }

    pub fn reset(&mut self) {
        self.last = None;
    }
}

fn signed_nanos(time: Duration) -> i128 {
    i128::from(time.as_secs()) * i128::from(NANOS_PER_SECOND) + i128::from(time.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::{Required, TestResult};

    fn frame(sequence: u64, animation: u64, display: u64) -> PresentedFrame {
        PresentedFrame {
            sequence,
            animation_time: Duration::from_millis(animation),
            presentation_time: Duration::from_millis(display),
        }
    }

    #[test]
    fn constant_latency_has_zero_animation_error() -> TestResult {
        let mut tracker = PresentationTracker::default();
        assert!(tracker.presented(frame(1, 0, 100)).is_none());
        let metric = tracker.presented(frame(2, 10, 110)).required("metric")?;
        assert_eq!(metric.animation_error_ns, 0);
        assert_eq!(metric.prediction_error_ns, -100_000_000);
        Ok(())
    }

    #[test]
    fn even_display_intervals_can_hide_bad_animation() -> TestResult {
        let mut tracker = PresentationTracker::default();
        assert!(tracker.presented(frame(1, 0, 0)).is_none());
        let early = tracker.presented(frame(2, 2, 10)).required("metric")?;
        assert_eq!(early.animation_error_ns, -8_000_000);
        let late = tracker.presented(frame(3, 20, 20)).required("metric")?;
        assert_eq!(late.animation_error_ns, 8_000_000);
        Ok(())
    }

    #[test]
    fn irregular_vrr_has_zero_error_when_animation_matches_display() {
        let mut tracker = PresentationTracker::default();
        for (sequence, time) in (0..).zip([0, 8, 23, 34, 51]) {
            if let Some(metric) = tracker.presented(frame(sequence, time, time + 50)) {
                assert_eq!(metric.animation_error_ns, 0);
            }
        }
    }

    #[test]
    fn displayed_pairs_span_discarded_submissions() -> TestResult {
        let mut tracker = PresentationTracker::default();
        assert!(tracker.presented(frame(1, 0, 5)).is_none());
        let metric = tracker.presented(frame(4, 30, 35)).required("metric")?;
        assert_eq!(metric.animation_step, Duration::from_millis(30));
        assert_eq!(metric.animation_error_ns, 0);
        Ok(())
    }

    #[test]
    fn duplicate_late_or_backwards_feedback_does_not_poison_the_pair() -> TestResult {
        let mut tracker = PresentationTracker::default();
        assert!(tracker.presented(frame(2, 10, 15)).is_none());
        for invalid in [
            frame(1, 0, 5),
            frame(2, 10, 15),
            frame(3, 9, 25),
            frame(3, 20, 15),
        ] {
            assert!(tracker.presented(invalid).is_none());
        }
        let metric = tracker.presented(frame(4, 30, 35)).required("metric")?;
        assert_eq!(metric.animation_error_ns, 0);
        tracker.reset();
        assert!(tracker.presented(frame(1, 0, 0)).is_none());
        Ok(())
    }
}
