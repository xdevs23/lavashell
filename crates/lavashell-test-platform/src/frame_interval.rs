// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInterval(Duration);

impl FrameInterval {
    #[must_use]
    pub fn new(interval: Duration) -> Option<Self> {
        (!interval.is_zero()).then_some(Self(interval))
    }

    #[must_use]
    pub fn get(self) -> Duration {
        self.0
    }
}
