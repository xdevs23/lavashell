// SPDX-License-Identifier: GPL-3.0-only
use super::protocol_layout::protocol_zone;
use super::*;
use crate::test_error::TestError;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins,
};
use lavashell_core::units::{Dp, DpExt, FiniteDp};
use zwlr_layer_surface_v1::Anchor;

fn margins(top: Dp, left: Dp) -> Result<LayerMargins, TestError> {
    Ok(LayerMargins {
        top: FiniteDp::try_new(top)?,
        left: FiniteDp::try_new(left)?,
        ..LayerMargins::default()
    })
}

fn fixed(length: Dp) -> Result<Extent, TestError> {
    Ok(Extent::dp(length)?)
}

fn measured(layout: LayerLayout, width: Dp, height: Dp) -> Result<ProtocolLayout, TestError> {
    let content = ContentSize::try_from(Size::new(width, height))?;
    Ok(ProtocolLayout::new(PanelGeometry::of(layout, content))?)
}

fn protocol(layout: LayerLayout) -> Result<ProtocolLayout, TestError> {
    measured(layout, Dp::ZERO, Dp::ZERO)
}

#[test]
fn layer_geometry_in_dp_becomes_whole_protocol_units() -> Result<(), TestError> {
    let layout = LayerLayout {
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(fixed(47.6.dp())?),
        exclusive_zone: ExclusiveZone::reserve(47.4.dp())?,
        margins: margins(-2.5.dp(), 0.4.dp())?,
    };
    let sent = protocol(layout)?;
    assert_eq!((sent.width, sent.height, sent.zone), (0, 48, 47));
    assert_eq!((sent.margins.top, sent.margins.left), (-3, 0));
    assert_eq!(protocol_zone(ExclusiveZone::IgnoreOthers, None)?, -1);
    Ok(())
}

#[test]
fn a_layer_length_below_one_logical_pixel_is_sent_as_one_and_never_as_fill() -> Result<(), TestError>
{
    let thin = LayerLayout {
        horizontal: AxisPlacement::Center(fixed(0.3.dp())?),
        vertical: AxisPlacement::Start(fixed(0.3.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins::default(),
    };
    let sent = protocol(thin)?;
    assert_eq!((sent.width, sent.height), (1, 1));
    Ok(())
}

#[test]
fn a_layer_length_too_long_for_the_protocol_is_refused() -> Result<(), TestError> {
    let filled = LayerLayout {
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Fill,
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins::default(),
    };
    let long = LayerLayout {
        vertical: AxisPlacement::End(fixed(1e12.dp())?),
        ..filled
    };
    let reserved = LayerLayout {
        exclusive_zone: ExclusiveZone::reserve(1e12.dp())?,
        ..filled
    };
    let moved = LayerLayout {
        margins: margins(-1e12.dp(), Dp::ZERO)?,
        ..filled
    };
    for layout in [long, reserved, moved] {
        assert!(protocol(layout).is_err());
    }
    Ok(())
}

#[test]
fn a_content_sized_layer_takes_and_reserves_the_measured_length() -> Result<(), TestError> {
    let anchored = |horizontal: AxisPlacement, vertical: AxisPlacement, height: Dp| {
        let layout = LayerLayout {
            margins: LayerMargins::default(),
            horizontal,
            vertical,
            exclusive_zone: ExclusiveZone::LayerExtent,
        };
        let sent = measured(layout, 500.dp(), height)?;
        Ok::<_, TestError>((sent.anchor, sent.width, sent.height, sent.zone))
    };
    let content = AxisPlacement::Start(Extent::Content);
    let top = Anchor::Top | Anchor::Left | Anchor::Right;
    assert_eq!(
        anchored(AxisPlacement::Fill, content, 61.6.dp())?,
        (top, 0, 62, 62)
    );
    assert_eq!(
        anchored(AxisPlacement::Fill, content, 0.3.dp())?,
        (top, 0, 1, 1)
    );
    let left = Anchor::Left | Anchor::Top | Anchor::Bottom;
    assert_eq!(
        anchored(content, AxisPlacement::Fill, 61.6.dp())?,
        (left, 500, 0, 500)
    );
    assert_eq!(
        anchored(content, content, 61.6.dp())?,
        (Anchor::Top | Anchor::Left, 500, 62, 0)
    );
    let centered = AxisPlacement::Center(Extent::Content);
    let column = Anchor::Top | Anchor::Bottom;
    assert_eq!(
        anchored(centered, AxisPlacement::Fill, 61.6.dp())?,
        (column, 500, 0, 0)
    );
    Ok(())
}
