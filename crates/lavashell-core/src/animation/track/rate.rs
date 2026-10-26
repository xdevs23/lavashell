// SPDX-License-Identifier: GPL-3.0-only
use super::Animation;
use crate::animation::Animatable;

/// The error returned by `Animation::playback_rate` for a rate that is not finite or not greater than zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlaybackRateError {
    #[error("the playback rate must be finite")]
    NotFinite,
    #[error("the playback rate must be greater than zero")]
    NotPositive,
}

impl<T: Animatable> Animation<T> {
    /// Sets the playback rate of the animation. The elapsed time continues from its current value, so the value
    /// of the animation does not jump.
    ///
    /// # Errors
    ///
    /// Fails when `rate` is not finite or not greater than zero.
    pub fn playback_rate(&self, rate: f64) -> Result<(), PlaybackRateError> {
        let rate = checked_rate(rate)?;
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

/// `rate` when it is finite and greater than zero.
pub(crate) const fn checked_rate(rate: f64) -> Result<f64, PlaybackRateError> {
    if !rate.is_finite() {
        Err(PlaybackRateError::NotFinite)
    } else if rate <= 0.0 {
        Err(PlaybackRateError::NotPositive)
    } else {
        Ok(rate)
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
