// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{FiniteDp, NonNegativeDp};
use std::num::TryFromIntError;

/// A failure of a layer surface. Each variant is one operation that can fail.
#[derive(Debug, thiserror::Error)]
pub enum LayerError {
    #[error("the compositor does not support layer-shell")]
    NoLayerShell,
    #[error("the layout of a new layer does not fit the protocol")]
    Layout(#[source] LayerLayoutError),
    #[error("the runtime made panel requests for the surface {0:?}, which has no layer role")]
    PanelWithoutLayer(SurfaceId),
}

/// A layout of a layer whose lengths do not fit the layer-shell protocol.
#[derive(Debug, thiserror::Error)]
pub enum LayerLayoutError {
    #[error("the layer length {0:?} does not fit the protocol")]
    Length(NonNegativeDp),
    #[error("the exclusive zone {0:?} does not fit the protocol")]
    ExclusiveZone(NonNegativeDp),
    #[error("the extent of a layer is too long to reserve")]
    ReserveExtent(#[source] TryFromIntError),
    #[error("the layer margin {0:?} does not fit the protocol")]
    Margin(FiniteDp),
}
