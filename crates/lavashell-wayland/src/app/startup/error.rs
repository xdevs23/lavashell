// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;

/// A window requested by the runtime that the compositor cannot show.
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("the runtime requested a window whose minimum size {0:?} does not fit the protocol")]
    MinimumSizeBeyondProtocol(Size),
    #[error("the compositor does not support xdg-shell")]
    NoXdgShell,
}
