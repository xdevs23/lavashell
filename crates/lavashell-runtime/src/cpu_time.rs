// SPDX-License-Identifier: GPL-3.0-only
use std::time::Duration;

/// A reading of the CPU clock of the thread that took it.
#[derive(Clone, Copy, Debug)]
pub struct CpuStamp(Duration);

impl CpuStamp {
    /// Reads the CPU clock of the calling thread, or returns `None` when the clock reports a negative time.
    #[must_use]
    pub fn now() -> Option<Self> {
        let stamp = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
        Duration::try_from(stamp).ok().map(Self)
    }

    /// The CPU time that the calling thread spent since the stamp. The stamp has to come from the calling thread.
    #[must_use]
    pub fn elapsed(self) -> Option<Duration> {
        Self::now().map(|now| now.0.saturating_sub(self.0))
    }
}
