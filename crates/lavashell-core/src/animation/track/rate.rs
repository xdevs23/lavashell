// SPDX-License-Identifier: GPL-3.0-only
use super::Animation;
use crate::animation::Animatable;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlaybackRateError {
    #[error("the playback rate must be finite")]
    NotFinite,
    #[error("the playback rate must be greater than zero")]
    NotPositive,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "f64", into = "f64")
)]
pub struct PlaybackRate(f64);

impl PlaybackRate {
    pub const ONE: Self = Self(1.0);

    pub const fn try_new(rate: f64) -> Result<Self, PlaybackRateError> {
        if !rate.is_finite() {
            Err(PlaybackRateError::NotFinite)
        } else if rate <= 0.0 {
            Err(PlaybackRateError::NotPositive)
        } else {
            Ok(Self(rate))
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for PlaybackRate {
    type Error = PlaybackRateError;

    fn try_from(rate: f64) -> Result<Self, PlaybackRateError> {
        Self::try_new(rate)
    }
}

impl From<PlaybackRate> for f64 {
    fn from(rate: PlaybackRate) -> Self {
        rate.get()
    }
}

impl<T: Animatable> Animation<T> {
    pub fn playback_rate(&self, rate: f64) -> Result<(), PlaybackRateError> {
        let rate = PlaybackRate::try_new(rate)?;
        self.0.synchronize();
        let mut t = self.0.track.borrow_mut();
        t.timing.offset = t.elapsed();
        t.timing.start = t.timing.paused.unwrap_or(t.now);
        t.timing.rate = rate;
        drop(t);
        self.0.revise();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Motion, Tween};
    use crate::float_asserts::assert_f64_eq;
    use crate::test_error::TestResult;
    use std::time::Duration;

    #[test]
    fn a_playback_rate_is_finite_and_greater_than_zero() -> TestResult {
        let animation = Animation::new(0.0_f64)?;
        assert_eq!(
            animation.playback_rate(f64::NAN),
            Err(PlaybackRateError::NotFinite)
        );
        assert_eq!(
            animation.playback_rate(f64::INFINITY),
            Err(PlaybackRateError::NotFinite)
        );
        assert_eq!(
            animation.playback_rate(0.0),
            Err(PlaybackRateError::NotPositive)
        );
        assert_eq!(
            animation.playback_rate(-1.0),
            Err(PlaybackRateError::NotPositive)
        );
        assert_eq!(animation.playback_rate(f64::MIN_POSITIVE), Ok(()));
        Ok(())
    }

    #[test]
    fn a_playback_rate_beyond_the_range_of_time_ends_the_program_instead_of_overflowing()
    -> TestResult {
        let motion = Motion::default();
        let animation = motion.animation(0.0_f64)?;
        animation.animate_to(1.0, Tween::new(Duration::from_secs(1)))?;
        animation.playback_rate(f64::MAX)?;
        motion.advance(Duration::from_secs(1));
        assert!(animation.is_finished());
        assert_f64_eq(animation.get(), 1.0);
        assert_eq!(animation.elapsed(), Duration::from_secs(1));
        Ok(())
    }
}
