// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

#[derive(Debug, thiserror::Error)]
pub enum FocusGrabError {
    #[error(
        "the runtime made a focus grab request, but the compositor does not support focus grabs"
    )]
    NoFocusGrab,
    #[error(
        "the runtime made a focus grab request for the surface {0:?}, which has no native surface"
    )]
    WithoutSurface(SurfaceId),
}
