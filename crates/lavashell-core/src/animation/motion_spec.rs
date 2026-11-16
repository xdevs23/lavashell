// SPDX-License-Identifier: GPL-3.0-only
use super::{Easing, Spring};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tween {
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    pub preserve_velocity: bool,
}

impl Tween {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            delay: Duration::ZERO,
            easing: Easing::Smoother,
            preserve_velocity: true,
        }
    }

    #[must_use]
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    #[must_use]
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    #[must_use]
    pub fn preserve_velocity(mut self, enabled: bool) -> Self {
        self.preserve_velocity = enabled;
        self
    }

    pub(crate) fn ends_after(&self) -> Duration {
        self.duration.saturating_add(self.delay)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MotionSpec {
    Instant,
    Tween(Tween),
    Spring(Spring),
}

impl MotionSpec {
    pub(crate) fn is_instant(self) -> bool {
        match self {
            Self::Instant => true,
            Self::Tween(tween) => tween.ends_after().is_zero(),
            Self::Spring(_) => false,
        }
    }
}

impl From<Spring> for MotionSpec {
    fn from(value: Spring) -> Self {
        Self::Spring(value)
    }
}

impl From<Tween> for MotionSpec {
    fn from(value: Tween) -> Self {
        Self::Tween(value)
    }
}
