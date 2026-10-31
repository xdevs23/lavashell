// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp};

/// What the host system did with a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    /// The host gave the surface a size. A length that the host leaves to the surface is `None`.
    Configured {
        width: Option<Dp>,
        height: Option<Dp>,
    },
    /// The surface closed, whatever the cause.
    Closed,
    DensityChanged(Density),
}

/// What the runtime asks a platform to do with a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRequest {
    /// Releases the native surface and everything that presents to it.
    Destroy(SurfaceId),
}
