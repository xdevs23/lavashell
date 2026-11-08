// SPDX-License-Identifier: GPL-3.0-only
use super::protocol_layout::{GeometryRequest, protocol_zone};
use super::*;
use crate::test_error::TestError;
use focus_wire::empty_view;
use lavashell_core::geometry::Size;
use lavashell_core::panel::{
    AxisPlacement, ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins,
};
use lavashell_core::reactive::State;
use lavashell_core::units::{Dp, DpExt, FiniteDp};
use lavashell_test_fixtures::NoText;
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

/// The protocol layout of `layout` for content of `width` by `height`.
fn measured(layout: LayerLayout, width: Dp, height: Dp) -> Result<ProtocolLayout, TestError> {
    let content = ContentSize::try_from(Size::new(width, height))?;
    Ok(ProtocolLayout::new(PanelGeometry::of(layout, content))?)
}

fn protocol(layout: LayerLayout) -> Result<ProtocolLayout, TestError> {
    measured(layout, Dp::ZERO, Dp::ZERO)
}

#[test]
fn moving_a_bounded_surface_changes_only_margins_without_a_resize_request() -> Result<(), TestError>
{
    let first = LayerLayout {
        horizontal: AxisPlacement::Start(fixed(320.dp())?),
        vertical: AxisPlacement::Start(fixed(344.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: margins(48.dp(), 1592.dp())?,
    };
    let moved = LayerLayout {
        margins: margins(48.dp(), -80.dp())?,
        ..first
    };
    assert_eq!(
        GeometryRequest::needed(Some(protocol(first)?), protocol(moved)?),
        [GeometryRequest::Margin]
    );
    let grown = LayerLayout {
        vertical: AxisPlacement::Start(fixed(400.dp())?),
        ..moved
    };
    assert_eq!(
        GeometryRequest::needed(Some(protocol(moved)?), protocol(grown)?),
        [GeometryRequest::Size]
    );
    assert!(GeometryRequest::needed(Some(protocol(grown)?), protocol(grown)?).is_empty());
    let nudged = LayerLayout {
        vertical: AxisPlacement::Start(fixed(400.3.dp())?),
        margins: margins(48.dp(), -80.4.dp())?,
        ..grown
    };
    assert!(GeometryRequest::needed(Some(protocol(grown)?), protocol(nudged)?).is_empty());
    let docked = LayerLayout {
        horizontal: AxisPlacement::End(fixed(320.dp())?),
        ..nudged
    };
    assert_eq!(
        GeometryRequest::needed(Some(protocol(nudged)?), protocol(docked)?),
        [GeometryRequest::Anchor]
    );
    // A layer on a reconnected output gets a new protocol object, so every geometry request is sent again in
    // place of only the requests that changed.
    assert_eq!(
        GeometryRequest::needed(None, protocol(docked)?),
        GeometryRequest::ALL
    );
    Ok(())
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
    let bar = LayerLayout {
        horizontal: AxisPlacement::Fill,
        ..thin
    };
    assert_eq!(initial_size(protocol(bar)?), (1, 1));
    Ok(())
}

#[test]
fn the_initial_size_of_a_layer_follows_the_layout_state_over_the_static_placement()
-> Result<(), TestError> {
    let layout = LayerLayout {
        horizontal: AxisPlacement::Start(fixed(321.dp())?),
        vertical: AxisPlacement::Start(fixed(345.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins::default(),
    };
    let short = AxisPlacement::Start(fixed(10.dp())?);
    let options = LayerSurface::new("stateful")
        .placement(AxisPlacement::Fill, short)
        .layout_state(State::new(layout));
    let panel =
        Panel::new(options.panel.clone(), &mut empty_view(), &mut NoText).map_err(Error::from)?;
    let protocol = ProtocolLayout::new(panel.geometry())?;
    let spec = NativeSpec::layer(&options, empty_view(), panel.geometry(), protocol);
    assert_eq!(spec.size, (321, 345));
    assert_eq!(spec.size, (protocol.width, protocol.height));
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
fn a_default_layer_takes_and_reserves_the_measured_height_of_its_content() -> Result<(), TestError>
{
    let anchored = |options: LayerSurface, height: Dp| {
        let sent = measured(options.panel.current_layout(), 500.dp(), height)?;
        Ok::<_, TestError>((sent.anchor, sent.width, sent.height, sent.zone))
    };
    let top = Anchor::Top | Anchor::Left | Anchor::Right;
    assert_eq!(
        anchored(LayerSurface::new("measured"), 61.6.dp())?,
        (top, 0, 62, 62)
    );
    assert_eq!(
        anchored(LayerSurface::new("thin"), 0.3.dp())?,
        (top, 0, 1, 1)
    );
    let side = LayerSurface::new("side")
        .placement(AxisPlacement::Start(Extent::Content), AxisPlacement::Fill);
    let left = Anchor::Left | Anchor::Top | Anchor::Bottom;
    assert_eq!(anchored(side, 61.6.dp())?, (left, 500, 0, 500));
    let corner = LayerSurface::new("corner").placement(
        AxisPlacement::Start(Extent::Content),
        AxisPlacement::Start(Extent::Content),
    );
    assert_eq!(
        anchored(corner, 61.6.dp())?,
        (Anchor::Top | Anchor::Left, 500, 62, 0)
    );
    let centered = LayerSurface::new("centered")
        .placement(AxisPlacement::Center(Extent::Content), AxisPlacement::Fill);
    let column = Anchor::Top | Anchor::Bottom;
    assert_eq!(anchored(centered, 61.6.dp())?, (column, 500, 0, 0));
    Ok(())
}

#[test]
fn an_axis_left_to_the_layer_by_a_configure_event_takes_the_length_requested_last()
-> Result<(), TestError> {
    let first = LayerLayout {
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(fixed(344.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins::default(),
    };
    let grown = LayerLayout {
        vertical: AxisPlacement::Start(fixed(400.dp())?),
        ..first
    };
    let requested = protocol(grown)?;
    assert_eq!(configured_length(0, requested.height), Some(400));
    assert_eq!(configured_length(380, requested.height), Some(380));
    assert_eq!(configured_length(1920, requested.width), Some(1920));
    assert_eq!(configured_length(0, requested.width), None);
    Ok(())
}
