// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// The time between two frames of a recording, which is longer than zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInterval(Duration);

/// A frame rate of zero, or of more than one frame per nanosecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a frame rate must be above zero and at most one frame per nanosecond")]
pub struct FrameRateError;

impl FrameInterval {
    #[must_use]
    pub fn new(interval: Duration) -> Option<Self> {
        (!interval.is_zero()).then_some(Self(interval))
    }

    /// The interval of `frames` frames per second, rounded down to whole nanoseconds. Inside `constant!` of the core
    /// crate, the compiler checks a literal rate while it builds the program.
    ///
    /// # Errors
    ///
    /// Fails with [`FrameRateError`] when `frames` is zero or above one frame per nanosecond.
    pub const fn per_second(frames: u64) -> Result<Self, FrameRateError> {
        match NANOS_PER_SECOND.checked_div(frames) {
            Some(nanos) if nanos > 0 => Ok(Self(Duration::from_nanos(nanos))),
            _ => Err(FrameRateError),
        }
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}
