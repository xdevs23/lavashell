// SPDX-License-Identifier: GPL-3.0-only
use crate::{KeyboardMode, OutputId};
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::geometry::Size;
use lavashell_core::output::{HdrMetadata, OutputRequest};
use lavashell_core::panel::{LayerLevel, PanelGeometry};
use lavashell_core::surface::{ConfigureToken, SurfaceId};
use lavashell_core::units::{Density, Dp};
use lavashell_renderer::{Attach, SurfaceAlpha};
use lavashell_vulkan::Window;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    Configured {
        width: Option<Dp>,
        height: Option<Dp>,
        token: Option<ConfigureToken>,
    },
    Closed,
    DensityChanged(Density),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceEventKey {
    Configuration,
    Density,
}

impl Merge for SurfaceEvent {
    type Key = SurfaceEventKey;

    fn effect(&self) -> MergeEffect<SurfaceEventKey> {
        match self {
            Self::Configured { .. } => MergeEffect::Overwrites(SurfaceEventKey::Configuration),
            Self::DensityChanged(_) => MergeEffect::Overwrites(SurfaceEventKey::Density),
            Self::Closed => MergeEffect::Appends,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceRequest {
    Create {
        surface: SurfaceId,
        spec: Box<SurfaceSpec>,
    },
    Destroy(SurfaceId),
    Release(SurfaceId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    pub size: Size,
    pub appearance: SurfaceAppearance,
    pub role: Role,
}

impl SurfaceSpec {
    #[must_use]
    pub fn attach<C>(&self, surface: SurfaceId, window: Arc<dyn Window>, control: C) -> Attach<C> {
        let alpha = if self.appearance.transparent {
            SurfaceAlpha::Transparent
        } else {
            SurfaceAlpha::Opaque
        };
        Attach {
            surface,
            window,
            alpha,
            output: self.appearance.output,
            hdr_metadata: self.appearance.hdr_metadata,
            control,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceAppearance {
    pub transparent: bool,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Window(WindowRole),
    Panel(PanelRole),
    Lock { output: OutputId },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowRole {
    pub title: String,
    pub app_id: String,
    pub min_size: Option<Size>,
    pub fullscreen: bool,
    pub draws_decorations: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelRole {
    pub output: Option<OutputId>,
    pub level: LayerLevel,
    pub namespace: String,
    pub geometry: PanelGeometry,
    pub keyboard: KeyboardMode,
}
