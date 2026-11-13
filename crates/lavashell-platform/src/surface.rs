// SPDX-License-Identifier: GPL-3.0-only
use crate::{KeyboardMode, OutputId};
use lavashell_core::geometry::Size;
use lavashell_core::output::{HdrMetadata, OutputRequest};
use lavashell_core::panel::{LayerLevel, PanelGeometry};
use lavashell_core::surface::{ConfigureToken, SurfaceId};
use lavashell_core::units::{Density, Dp};
use lavashell_renderer::{Attach, PresentationPolicy, SurfaceAlpha, SurfaceTiming};
use lavashell_vulkan::Window;
use std::sync::Arc;
use std::time::Duration;

/// What the host system did with a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    /// The host gave the surface a size. A length that the host leaves to the surface is `None`. A host that expects
    /// the acknowledgement of a configure together with the frame that applies it reports `token`, which the surface
    /// state of that frame contains.
    Configured {
        width: Option<Dp>,
        height: Option<Dp>,
        token: Option<ConfigureToken>,
    },
    /// The surface closed, whatever the cause.
    Closed,
    DensityChanged(Density),
}

/// What the runtime asks a platform to do with a surface.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceRequest {
    /// Creates the native surface of `surface` as `spec` describes, and attaches it to the renderer. The effects of
    /// the surface get `gpu_budget` as their GPU time per frame, or no limit for `None`.
    Create {
        surface: SurfaceId,
        spec: Box<SurfaceSpec>,
        gpu_budget: Option<Duration>,
    },
    /// Releases the native surface and everything that presents to it. The platform reports no more events of the
    /// surface and detaches it from the renderer, and destroys the native surface on `Release`.
    Destroy(SurfaceId),
    /// Destroys the native surface of a destroyed surface, which the renderer retired.
    Release(SurfaceId),
}

/// The native surface created by a platform for a surface of the runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    /// The size in dp at which the surface opens.
    pub size: Size,
    pub appearance: Appearance,
    pub role: Role,
}

impl SurfaceSpec {
    /// What attaches `surface` to the renderer, presented on `window` as the spec describes, with `gpu_budget` and
    /// `control`.
    #[must_use]
    pub fn attach<C>(
        &self,
        surface: SurfaceId,
        window: Arc<dyn Window>,
        gpu_budget: Option<Duration>,
        control: C,
    ) -> Attach<C> {
        Attach {
            surface,
            window,
            alpha: if self.appearance.transparent {
                SurfaceAlpha::Transparent
            } else {
                SurfaceAlpha::Opaque
            },
            timing: SurfaceTiming::new(PresentationPolicy::Automatic),
            output: self.appearance.output,
            hdr_metadata: self.appearance.hdr_metadata,
            gpu_budget,
            control,
        }
    }
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
    Window(WindowRole),
    Panel(PanelRole),
    /// The surface that the session lock shows on `output`.
    Lock {
        output: OutputId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowRole {
    pub title: String,
    /// The id of the app, by which the host groups its windows and finds its name and icon.
    pub app_id: String,
    /// The size in dp below which the window does not shrink.
    pub min_size: Option<Size>,
    pub fullscreen: bool,
    /// Whether the window draws its decorations. Otherwise the host draws them.
    pub draws_decorations: bool,
}

/// A surface that the host places at the edges of an output, such as a bar or a dock.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelRole {
    /// The output that shows the panel, or `None` for the output chosen by the host.
    pub output: Option<OutputId>,
    pub level: LayerLevel,
    /// The name used by the host to distinguish the panels of different apps.
    pub namespace: String,
    pub geometry: PanelGeometry,
    pub keyboard: KeyboardMode,
}
