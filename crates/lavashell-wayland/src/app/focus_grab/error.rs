// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;

/// A focus grab request from the runtime that cannot be carried out.
#[derive(Debug, thiserror::Error)]
pub enum FocusGrabError {
    #[error(
        "the runtime made a focus grab request for the surface {0:?}, which has no native surface"
    )]
    WithoutSurface(SurfaceId),
}
