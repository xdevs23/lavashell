// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawnFrame, SkippedFrame};
use crate::Error;
use lavashell_core::prelude::PixelSize;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PresentationPolicy {
    Automatic,
    Explicit,
}

impl PresentationPolicy {
    fn check(self, time: Option<PresentationTime>) -> Result<(), Error> {
        if self == Self::Explicit && time.is_none() {
            return Err(Error::ExplicitTimestampRequired);
        }
        Ok(())
    }

    pub(super) fn after_submission(self, time: Option<PresentationTime>) -> Self {
        match time {
            Some(_) => Self::Explicit,
            None => self,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::vulkan) struct PresentationTime(Duration);

impl PresentationTime {
    const LATEST: Duration = Duration::from_nanos(i64::MAX.unsigned_abs());

    pub(in crate::vulkan) fn new(time: Duration) -> Result<Self, Error> {
        if time.is_zero() || time > Self::LATEST {
            return Err(Error::PresentationTimeRange(time));
        }
        Ok(Self(time))
    }

    pub(super) fn time(self) -> Duration {
        self.0
    }
}

pub(super) fn before_acquisition<T>(
    policy: PresentationPolicy,
    size: PixelSize,
    time: Option<PresentationTime>,
    acquire: impl FnOnce() -> Result<DrawnFrame<T>, Error>,
) -> Result<DrawnFrame<T>, Error> {
    if size.width == 0 || size.height == 0 {
        return Ok(DrawnFrame::Skipped(SkippedFrame::EmptySize));
    }
    policy.check(time)?;
    acquire()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn explicit_times_reject_auto_and_signed_overflow() {
        let beyond = Duration::from_nanos(i64::MAX.unsigned_abs() + 1);
        for time in [Duration::ZERO, beyond, Duration::MAX] {
            assert!(matches!(
                PresentationTime::new(time),
                Err(Error::PresentationTimeRange(rejected)) if rejected == time
            ));
        }
    }

    #[test]
    fn explicit_times_keep_every_signed_nanosecond_value() -> Result<(), TestError> {
        for nanos in [1, 23_456_789, i64::MAX.unsigned_abs()] {
            let time = Duration::from_nanos(nanos);
            assert_eq!(PresentationTime::new(time)?.time(), time);
        }
        Ok(())
    }

    #[test]
    fn pending_checks_do_not_latch_a_time_and_explicit_submissions_require_future_times()
    -> Result<(), TestError> {
        let policy = PresentationPolicy::Automatic;
        let time = Some(PresentationTime::new(Duration::from_nanos(5_000))?);
        policy.check(time)?;
        policy.check(None)?;
        assert_eq!(policy.after_submission(None), PresentationPolicy::Automatic);
        let policy = policy.after_submission(time);
        assert!(matches!(
            policy.check(None),
            Err(Error::ExplicitTimestampRequired)
        ));
        let later = PresentationTime::new(Duration::from_micros(19))?;
        policy.check(Some(later))?;
        Ok(())
    }
}
