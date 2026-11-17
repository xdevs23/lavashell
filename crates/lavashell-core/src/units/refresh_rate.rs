// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

const MILLIHERTZ_PER_HERTZ: f64 = 1000.0;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct RefreshRate {
    hertz: f64,
}

impl RefreshRate {
    #[must_use]
    pub const fn from_hertz(hertz: f64) -> Self {
        Self { hertz }
    }

    #[must_use]
    pub fn from_millihertz(millihertz: i32) -> Self {
        Self::from_hertz(f64::from(millihertz) / MILLIHERTZ_PER_HERTZ)
    }

    #[must_use]
    pub fn interval(self) -> Option<Duration> {
        let interval = Duration::try_from_secs_f64(self.hertz.recip()).ok()?;
        (!interval.is_zero()).then_some(interval)
    }
}
