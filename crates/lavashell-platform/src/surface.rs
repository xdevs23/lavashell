// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::geometry::Size;
use lavashell_core::output::{HdrMetadata, OutputRequest};
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

/// The native surface created by a platform for a surface of the runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    /// The size in dp at which the surface opens.
    pub size: Size,
    pub appearance: Appearance,
    pub role: Role,
}

/// How the host shows the content of a surface, whatever its role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    /// Whether the content has an alpha channel, through which the host shows what lies behind the surface.
    pub transparent: bool,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    /// Whether the host blurs what is behind the whole surface.
    pub background_blur: bool,
}

/// What a surface is to the host system.
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Window {
        title: String,
        /// The id of the app, by which the host groups its windows and finds its name and icon.
        app_id: String,
        /// The size in dp below which the window does not shrink.
        min_size: Option<Size>,
        fullscreen: bool,
        /// Whether the window draws its decorations. Otherwise the host draws them.
        draws_decorations: bool,
    },
}
