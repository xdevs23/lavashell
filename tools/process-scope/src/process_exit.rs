// SPDX-License-Identifier: GPL-3.0-only
use rustix::process::WaitIdStatus;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
    Code(i32),
    Signal(i32),
}

impl ProcessExit {
    #[must_use]
    pub fn success(self) -> bool {
        self == Self::Code(0)
    }

    pub(crate) fn read(status: &WaitIdStatus) -> Option<Self> {
        (status.exit_status().map(Self::Code))
            .or_else(|| status.terminating_signal().map(Self::Signal))
    }
}

impl fmt::Display for ProcessExit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(code) => write!(formatter, "exited with code {code}"),
            Self::Signal(signal) => write!(formatter, "was ended by signal {signal}"),
        }
    }
}
