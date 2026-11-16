// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CpuTiming {
    Measured,
    Skipped,
}

impl CpuTiming {
    #[must_use]
    pub fn stamp(self) -> Option<CpuStamp> {
        match self {
            Self::Measured => CpuStamp::now(),
            Self::Skipped => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CpuStamp(Duration);

impl CpuStamp {
    #[must_use]
    pub fn now() -> Option<Self> {
        let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
        Duration::try_from(stamp).ok().map(Self)
    }

    #[must_use]
    pub fn elapsed(self) -> Option<Duration> {
        Self::now().map(|now| now.0.saturating_sub(self.0))
    }
}
