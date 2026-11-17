// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInterval(Duration);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a frame rate must be above zero and at most one frame per nanosecond")]
pub struct FrameRateError;

impl FrameInterval {
    #[must_use]
    pub fn new(interval: Duration) -> Option<Self> {
        (!interval.is_zero()).then_some(Self(interval))
    }

    pub const fn per_second(frames: u32) -> Result<Self, FrameRateError> {
        match Duration::from_secs(1).checked_div(frames) {
            Some(interval) if !interval.is_zero() => Ok(Self(interval)),
            _ => Err(FrameRateError),
        }
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}
