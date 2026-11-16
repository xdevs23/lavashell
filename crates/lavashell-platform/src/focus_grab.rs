// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusGrabRequest {
    Acquire(SurfaceId),
    Release(SurfaceId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusGrabEvent {
    Cleared(SurfaceId),
}
