// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameOutcome, SurfaceTiming};
use crate::Error;

/// A desired presentation time in nanoseconds on the clock of `VK_GOOGLE_display_timing`.
#[derive(Clone, Copy)]
pub(in crate::vulkan) struct PresentationTime(u64);

impl PresentationTime {
    #[cfg(any(target_os = "android", test))]
    pub(in crate::vulkan) fn new(nanos: u64, supported: bool) -> Result<Self, Error> {
        if !supported {
            return Err(Error::NoDisplayTiming);
        }
        if nanos == 0 || i64::try_from(nanos).is_err() {
            return Err(Error::PresentationTimeRange(nanos));
        }
        Ok(Self(nanos))
    }

    pub(super) fn nanos(self) -> u64 {
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
        for time in [0, i64::MAX.unsigned_abs() + 1, u64::MAX] {
            assert!(matches!(
                PresentationTime::new(time, true),
                Err(Error::PresentationTimeRange(rejected)) if rejected == time
            ));
        }
    }

    #[test]
    fn explicit_times_need_a_device_with_display_timing() {
        assert!(matches!(
            PresentationTime::new(1, false),
            Err(Error::NoDisplayTiming)
        ));
    }

    #[test]
    fn explicit_times_keep_every_signed_nanosecond_value() -> Result<(), TestError> {
        for time in [1, 23_456_789, i64::MAX.unsigned_abs()] {
            assert_eq!(PresentationTime::new(time, true)?.nanos(), time);
        }
        Ok(())
    }

    #[test]
    fn pending_checks_do_not_latch_a_time_and_explicit_submissions_require_future_times()
    -> Result<(), TestError> {
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        let time = Some(PresentationTime::new(5_000, true)?);
        timing.check(time.is_some())?;
        timing.check(false)?;
        timing.submitting(time.is_some());
        assert!(matches!(
            timing.check(false),
            Err(Error::ExplicitTimestampRequired)
        ));
        timing.check(Some(PresentationTime::new(19_000, true)?).is_some())?;
        Ok(())
    }
}
