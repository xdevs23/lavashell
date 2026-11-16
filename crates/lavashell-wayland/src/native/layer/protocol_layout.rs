// SPDX-License-Identifier: GPL-3.0-only
use crate::LayerLayoutError;
use crate::logical::{round_to_i32, round_to_u32};
use crate::sctk;
use lavashell_core::panel::{AxisPlacement, ExclusiveZone, PanelGeometry};
use lavashell_core::units::{FiniteDp, NonNegativeDp};
use sctk::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    Anchor, ZwlrLayerSurfaceV1,
};

// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml
const SIZE_FROM_ANCHORS: u32 = 0;
// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml
const NO_EXCLUSIVE_ZONE: i32 = 0;
// https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml
const EXCLUSIVE_ZONE_IGNORING_OTHERS: i32 = -1;

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

fn protocol_extent(span: AxisPlacement<NonNegativeDp>) -> Result<u32, LayerLayoutError> {
    match span.extent() {
        None => Ok(SIZE_FROM_ANCHORS),
        Some(length) => round_to_u32(length.get()).ok_or(LayerLayoutError::Length(length)),
    }
}

pub(super) fn protocol_zone(
    zone: ExclusiveZone,
    across: Option<u32>,
) -> Result<i32, LayerLayoutError> {
    match zone {
        ExclusiveZone::Reserve(length) => {
            round_to_i32(length.get()).ok_or(LayerLayoutError::ExclusiveZone(length))
        }
        ExclusiveZone::LayerExtent => across
            .map_or(Ok(NO_EXCLUSIVE_ZONE), i32::try_from)
            .map_err(LayerLayoutError::ReserveExtent),
        ExclusiveZone::IgnoreOthers => Ok(EXCLUSIVE_ZONE_IGNORING_OTHERS),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::native) struct ProtocolLayout {
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
    pub(in crate::native) fn new(geometry: PanelGeometry) -> Result<Self, LayerLayoutError> {
        let margin =
            |length: FiniteDp| round_to_i32(length.get()).ok_or(LayerLayoutError::Margin(length));
        let margins = geometry.margins();
        let (horizontal, vertical) = (geometry.horizontal(), geometry.vertical());
        let width = protocol_extent(horizontal)?;
        let height = protocol_extent(vertical)?;
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
    if differs(previous, next, |layout| (layout.width, layout.height)) {
        proxy.set_size(next.width, next.height);
    }
    if differs(previous, next, |layout| layout.anchor) {
        proxy.set_anchor(next.anchor);
    }
    if differs(previous, next, |layout| layout.zone) {
        proxy.set_exclusive_zone(next.zone);
    }
    if differs(previous, next, |layout| layout.margins) {
        let ProtocolMargins {
            top,
            right,
            bottom,
            left,
        } = next.margins;
        proxy.set_margin(top, right, bottom, left);
    }
}

fn differs<T: PartialEq>(
    previous: Option<ProtocolLayout>,
    next: ProtocolLayout,
    part: impl Fn(ProtocolLayout) -> T,
) -> bool {
    previous.is_none_or(|previous| part(previous) != part(next))
}
