// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

/// What the runtime asks a platform to do with the focus grab of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusGrabRequest {
    /// Limits input focus to the surface until the host clears the grab or the runtime releases it.
    Acquire(SurfaceId),
    /// Ends the grab of the surface.
    Release(SurfaceId),
}

/// What the host did with the focus grab of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusGrabEvent {
    /// The host cleared the grab of the surface, such as when the user interacted outside it.
    Cleared(SurfaceId),
}
