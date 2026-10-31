// SPDX-License-Identifier: GPL-3.0-only
/// The state of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    /// The host has not configured the surface yet.
    Waiting,
    Configured,
    /// The surface closed, and the platform has not destroyed the native surface yet.
    Closing,
    /// The native surface was destroyed and the view was dropped. The record still exists, so the id is known as
    /// closed.
    Retired,
}

impl Lifecycle {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Waiting | Self::Configured)
    }

    pub fn is_configured(self) -> bool {
        self == Self::Configured
    }
}
