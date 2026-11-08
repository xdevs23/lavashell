// SPDX-License-Identifier: GPL-3.0-only
use crate::LayerLayoutError;
use crate::logical::{extent, whole};
use lavashell_core::panel::{AxisPlacement, ExclusiveZone, PanelGeometry};
use lavashell_core::units::{FiniteDp, NonNegativeDp};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, ZwlrLayerSurfaceV1,
};
use smithay_client_toolkit as sctk;

/// The edges of the output to which a layer is anchored along one axis, where `start` and `end` are the two edges of
/// that axis.
fn protocol_anchor(span: AxisPlacement<NonNegativeDp>, start: Anchor, end: Anchor) -> Anchor {
    match span {
        AxisPlacement::Start(_) => start,
        AxisPlacement::End(_) => end,
        AxisPlacement::Center(_) => Anchor::empty(),
        AxisPlacement::Fill => start | end,
    }
}

fn anchored_to_one_edge(span: AxisPlacement<NonNegativeDp>) -> bool {
    match span {
        AxisPlacement::Start(_) | AxisPlacement::End(_) => true,
        AxisPlacement::Center(_) | AxisPlacement::Fill => false,
    }
}

/// The size of a layer before the first configure event arrives, computed from the layout sent with its role. An
/// axis that fills starts at one logical pixel.
pub(super) fn initial_size(layout: ProtocolLayout) -> (u32, u32) {
    (layout.width.max(1), layout.height.max(1))
}

/// The length that the layer-shell protocol expects along one axis. The protocol expects zero for an
/// axis that fills the space between its anchors.
fn protocol_extent(span: AxisPlacement<NonNegativeDp>) -> Result<u32, LayerLayoutError> {
    match span.extent() {
        None => Ok(0),
        Some(length) => extent(length.get(), 0).ok_or(LayerLayoutError::Length(length)),
    }
}

/// The exclusive zone that the layer-shell protocol expects. The protocol expects -1 for a layer that
/// extends under the zones reserved by other layers. `across` is the protocol length of the layer
/// across its anchored edge, or `None` when the layer is not anchored to a single edge.
pub(super) fn protocol_zone(
    zone: ExclusiveZone,
    across: Option<u32>,
) -> Result<i32, LayerLayoutError> {
    match zone {
        ExclusiveZone::Reserve(length) => {
            whole(length.get()).ok_or(LayerLayoutError::ExclusiveZone(length))
        }
        ExclusiveZone::LayerExtent => {
            i32::try_from(across.unwrap_or(0)).map_err(LayerLayoutError::ReserveExtent)
        }
        ExclusiveZone::IgnoreOthers => Ok(-1),
    }
}

/// The whole numbers sent to the compositor for a layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app) struct ProtocolLayout {
    pub(super) anchor: Anchor,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) zone: i32,
    pub(super) margins: ProtocolMargins,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ProtocolMargins {
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
    pub(super) left: i32,
}

impl ProtocolLayout {
    pub(in crate::app) fn new(geometry: PanelGeometry) -> Result<Self, LayerLayoutError> {
        let margin = |length: FiniteDp| whole(length.get()).ok_or(LayerLayoutError::Margin(length));
        let margins = geometry.margins();
        let (horizontal, vertical) = (geometry.horizontal(), geometry.vertical());
        let width = protocol_extent(horizontal)?;
        let height = protocol_extent(vertical)?;
        // The compositor ignores the exclusive zone of a layer that is not anchored to a single edge.
        let across = match (
            anchored_to_one_edge(horizontal),
            anchored_to_one_edge(vertical),
        ) {
            (false, true) => Some(height),
            (true, false) => Some(width),
            _ => None,
        };
        Ok(Self {
            anchor: protocol_anchor(horizontal, Anchor::Left, Anchor::Right)
                | protocol_anchor(vertical, Anchor::Top, Anchor::Bottom),
            width,
            height,
            zone: protocol_zone(geometry.exclusive_zone(), across)?,
            margins: ProtocolMargins {
                top: margin(margins.top)?,
                right: margin(margins.right)?,
                bottom: margin(margins.bottom)?,
                left: margin(margins.left)?,
            },
        })
    }
}

pub(super) fn apply(
    proxy: &ZwlrLayerSurfaceV1,
    previous: Option<ProtocolLayout>,
    next: ProtocolLayout,
) {
    for request in GeometryRequest::needed(previous, next) {
        request.send(proxy, next);
    }
}

/// A request of the layer surface protocol that sets one part of the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeometryRequest {
    Size,
    Anchor,
    ExclusiveZone,
    Margin,
}

impl GeometryRequest {
    /// Every geometry request, in the order the layer-shell protocol declares them.
    pub(super) const ALL: [Self; 4] = [Self::Size, Self::Anchor, Self::ExclusiveZone, Self::Margin];

    /// The requests whose values differ between the layouts, or every request for a new protocol object.
    pub(super) fn needed(previous: Option<ProtocolLayout>, next: ProtocolLayout) -> Vec<Self> {
        (Self::ALL.into_iter())
            .filter(|request| previous.is_none_or(|previous| request.differs(previous, next)))
            .collect()
    }

    fn differs(self, previous: ProtocolLayout, next: ProtocolLayout) -> bool {
        match self {
            Self::Size => (previous.width, previous.height) != (next.width, next.height),
            Self::Anchor => previous.anchor != next.anchor,
            Self::ExclusiveZone => previous.zone != next.zone,
            Self::Margin => previous.margins != next.margins,
        }
    }

    fn send(self, proxy: &ZwlrLayerSurfaceV1, layout: ProtocolLayout) {
        match self {
            Self::Size => proxy.set_size(layout.width, layout.height),
            Self::Anchor => proxy.set_anchor(layout.anchor),
            Self::ExclusiveZone => proxy.set_exclusive_zone(layout.zone),
            Self::Margin => {
                let ProtocolMargins {
                    top,
                    right,
                    bottom,
                    left,
                } = layout.margins;
                proxy.set_margin(top, right, bottom, left);
            }
        }
    }
}
