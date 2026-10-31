// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameOutcome, SurfaceTiming};
use crate::Error;
use std::time::Duration;

/// A desired presentation time on the clock of `VK_GOOGLE_display_timing`, from 1 to `i64::MAX` nanoseconds.
#[derive(Clone, Copy)]
pub(in crate::vulkan) struct PresentationTime(Duration);

impl PresentationTime {
    const LATEST: Duration = Duration::from_nanos(i64::MAX.unsigned_abs());

    pub(in crate::vulkan) fn new(time: Duration, supported: bool) -> Result<Self, Error> {
        if !supported {
            return Err(Error::NoDisplayTiming);
        }
        if time.is_zero() || time > Self::LATEST {
            return Err(Error::PresentationTimeRange(time));
        }
        Ok(Self(time))
    }

    pub(super) fn time(self) -> Duration {
        self.0
    }
}

pub(in crate::vulkan) fn before_acquisition(
    timing: &SurfaceTiming,
    size: (u32, u32),
    time: Option<PresentationTime>,
    acquire: impl FnOnce() -> Result<FrameOutcome, Error>,
) -> Result<FrameOutcome, Error> {
    if size.0 == 0 || size.1 == 0 {
        return Ok(FrameOutcome::Pending);
    }
    timing.check(time.is_some())?;
    acquire()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;
    use crate::vulkan::PresentationPolicy;

    #[test]
    fn explicit_times_reject_auto_and_signed_overflow() {
        let beyond = Duration::from_nanos(i64::MAX.unsigned_abs() + 1);
        for time in [Duration::ZERO, beyond, Duration::MAX] {
            assert!(matches!(
                PresentationTime::new(time, true),
                Err(Error::PresentationTimeRange(rejected)) if rejected == time
            ));
        }
    }

    #[test]
    fn explicit_times_need_a_device_with_display_timing() {
        assert!(matches!(
            PresentationTime::new(Duration::from_nanos(1), false),
            Err(Error::NoDisplayTiming)
        ));
    }

    #[test]
    fn explicit_times_keep_every_signed_nanosecond_value() -> Result<(), TestError> {
        for nanos in [1, 23_456_789, i64::MAX.unsigned_abs()] {
            let time = Duration::from_nanos(nanos);
            assert_eq!(PresentationTime::new(time, true)?.time(), time);
        }
        Ok(())
    }

    #[test]
    fn pending_checks_do_not_latch_a_time_and_explicit_submissions_require_future_times()
    -> Result<(), TestError> {
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        let time = Some(PresentationTime::new(Duration::from_nanos(5_000), true)?);
        timing.check(time.is_some())?;
        timing.check(false)?;
        timing.submitting(time.is_some());
        assert!(matches!(
            timing.check(false),
            Err(Error::ExplicitTimestampRequired)
        ));
        let later = PresentationTime::new(Duration::from_micros(19), true)?;
        timing.check(Some(later).is_some())?;
        Ok(())
    }
}
