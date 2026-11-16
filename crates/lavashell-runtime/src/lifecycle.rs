// SPDX-License-Identifier: GPL-3.0-only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    Waiting,
    Configured,
    Closing,
}

impl Lifecycle {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Waiting | Self::Configured)
    }

    pub fn is_configured(self) -> bool {
        self == Self::Configured
    }
}
