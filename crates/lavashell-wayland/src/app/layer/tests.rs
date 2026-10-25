// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::LayerMargins;
use crate::test_error::TestError;
use lavashell_core::units::{DpExt, NumberError};

const UNMEASURED: LayerContext = LayerContext {
    content: Size::ZERO,
};

#[test]
fn moving_a_bounded_surface_changes_only_margins_without_a_resize_request() -> Result<(), TestError>
{
    let first = LayerLayout {
        horizontal: AxisPlacement::Start(Extent::dp(320.dp())?),
        vertical: AxisPlacement::Start(Extent::dp(344.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins {
            top: FiniteDp::try_new(48.dp())?,
            left: FiniteDp::try_new(1592.dp())?,
            ..Default::default()
        },
    };
    let moved = LayerLayout {
        margins: LayerMargins {
            left: FiniteDp::try_new(-80.dp())?,
            ..first.margins
        },
        ..first
    };
    let protocol = |layout| ProtocolLayout::new(layout, UNMEASURED);
    assert_eq!(
        GeometryRequest::needed(Some(protocol(first)?), protocol(moved)?),
        [GeometryRequest::Margin]
    );
    let grown = LayerLayout {
        vertical: AxisPlacement::Start(Extent::dp(400.dp())?),
        ..moved
    };
    assert_eq!(
        GeometryRequest::needed(Some(protocol(moved)?), protocol(grown)?),
        [GeometryRequest::Size]
    );
    assert!(GeometryRequest::needed(Some(protocol(grown)?), protocol(grown)?).is_empty());
    let nudged = LayerLayout {
        vertical: AxisPlacement::Start(Extent::dp(400.3.dp())?),
        margins: LayerMargins {
            left: FiniteDp::try_new(-80.4.dp())?,
            ..grown.margins
        },
        ..grown
    };
    assert!(GeometryRequest::needed(Some(protocol(grown)?), protocol(nudged)?).is_empty());
    let docked = LayerLayout {
        horizontal: AxisPlacement::End(Extent::dp(320.dp())?),
        ..nudged
    };
    assert_eq!(
        GeometryRequest::needed(Some(protocol(nudged)?), protocol(docked)?),
        [GeometryRequest::Anchor]
    );
    Ok(())
}

#[test]
fn each_native_lifecycle_applies_the_latest_state_and_all_protocol_geometry()
-> Result<(), TestError> {
    let layout = State::new(LayerLayout {
        horizontal: AxisPlacement::Start(Extent::dp(321.dp())?),
        vertical: AxisPlacement::Start(Extent::dp(1.dp())?),
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins {
            left: FiniteDp::try_new(1592.dp())?,
            top: FiniteDp::try_new(48.dp())?,
            ..Default::default()
        },
    });
    let options = LayerSurface::new("geometry-test").layout_state(layout.clone());
    let taller = AxisPlacement::Start(Extent::dp(345.dp())?);
    let left = FiniteDp::try_new(-327.dp())?;
    layout.update(|value| {
        value.vertical = taller;
        value.margins.left = left;
    });
    let configured = options.current_layout();
    assert_eq!(
        (configured.horizontal, configured.vertical),
        (
            AxisPlacement::Start(Extent::dp(321.dp())?),
            AxisPlacement::Start(Extent::dp(345.dp())?)
        )
    );
    assert_eq!(configured.margins.left.get(), -327.dp());
    assert_eq!(
        GeometryRequest::needed(None, ProtocolLayout::new(configured, UNMEASURED)?),
        GeometryRequest::ALL
    );
    // A layer on a reconnected output gets a new protocol object, so every geometry request is sent again in
    // place of only the requests that changed.
    assert_eq!(
        GeometryRequest::needed(
            None,
            ProtocolLayout::new(options.current_layout(), UNMEASURED)?
        ),
        GeometryRequest::ALL
    );
    Ok(())
}

#[test]
fn keyboard_activation_preserves_static_configuration_and_cannot_enable_a_disabled_layer() {
    assert!(!LayerSurface::new("default").keyboard_enabled());
    assert!(
        LayerSurface::new("interactive")
            .keyboard(true)
            .keyboard_enabled()
    );
    assert!(
        !LayerSurface::new("passthrough")
            .keyboard_when(|| true)
            .keyboard_enabled()
    );
}

#[test]
fn layer_geometry_in_dp_rounds_to_whole_protocol_units() -> Result<(), TestError> {
    let layout = LayerLayout {
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Start(Extent::dp(47.6.dp())?),
        exclusive_zone: ExclusiveZone::reserve(47.4.dp())?,
        margins: LayerMargins {
            top: FiniteDp::try_new(-2.5.dp())?,
            left: FiniteDp::try_new(0.4.dp())?,
            ..Default::default()
        },
    };
    assert_eq!(protocol_extent(layout.horizontal, Dp::ZERO)?, 0);
    assert_eq!(protocol_extent(layout.vertical, Dp::ZERO)?, 48);
    assert_eq!(protocol_zone(layout.exclusive_zone, None)?, 47);
    assert_eq!(protocol_zone(ExclusiveZone::IgnoreOthers, None)?, -1);
    let margins = ProtocolLayout::new(layout, UNMEASURED)?.margins;
    assert_eq!((margins.top, margins.left), (-3, 0));
    Ok(())
}

#[test]
fn a_layer_length_below_one_logical_pixel_is_sent_as_one_and_never_as_fill() -> Result<(), TestError>
{
    let short = AxisPlacement::Start(Extent::dp(0.3.dp())?);
    assert_eq!(protocol_extent(short, Dp::ZERO)?, 1);
    let content = AxisPlacement::Center(Extent::Content);
    assert_eq!(protocol_extent(content, 0.3.dp())?, 1);
    let options = LayerSurface::new("initial").placement(AxisPlacement::Fill, short);
    assert_eq!(initial_size(&options, UNMEASURED)?, (1, 1));
    Ok(())
}

#[test]
fn a_layer_length_that_is_not_finite_is_rejected() {
    assert_eq!(Extent::dp(Dp::new(f64::NAN)), Err(NumberError::NotFinite));
    assert_eq!(
        ExclusiveZone::reserve(Dp::new(f64::INFINITY)),
        Err(NumberError::NotFinite)
    );
}

#[test]
fn a_negative_layer_length_is_rejected() {
    assert_eq!(Extent::dp(-4.dp()), Err(NumberError::Negative));
    assert_eq!(ExclusiveZone::reserve(-4.dp()), Err(NumberError::Negative));
}

#[test]
fn a_layer_length_too_long_for_the_protocol_is_refused() -> Result<(), TestError> {
    let long = AxisPlacement::End(Extent::dp(1e12.dp())?);
    assert!(protocol_extent(long, Dp::ZERO).is_err());
    let content = AxisPlacement::Start(Extent::Content);
    assert!(protocol_extent(content, 1e12.dp()).is_err());
    assert!(protocol_zone(ExclusiveZone::reserve(1e12.dp())?, None).is_err());
    let layout = LayerLayout {
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Fill,
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins {
            top: FiniteDp::try_new(-1e12.dp())?,
            ..Default::default()
        },
    };
    assert!(ProtocolLayout::new(layout, UNMEASURED).is_err());
    Ok(())
}

#[test]
fn a_default_layer_takes_and_reserves_the_measured_height_of_its_content() -> Result<(), TestError>
{
    let options = LayerSurface::new("measured");
    let context = LayerContext {
        content: Size::new(500.dp(), 61.6.dp()),
    };
    let protocol = ProtocolLayout::new(options.current_layout(), context)?;
    assert_eq!(
        (
            protocol.anchor,
            protocol.width,
            protocol.height,
            protocol.zone
        ),
        (Anchor::Top | Anchor::Left | Anchor::Right, 0, 62, 62)
    );
    let side = LayerSurface::new("side")
        .placement(AxisPlacement::Start(Extent::Content), AxisPlacement::Fill);
    let protocol = ProtocolLayout::new(side.current_layout(), context)?;
    assert_eq!(
        (
            protocol.anchor,
            protocol.width,
            protocol.height,
            protocol.zone
        ),
        (Anchor::Left | Anchor::Top | Anchor::Bottom, 500, 0, 500)
    );
    let corner = LayerSurface::new("corner").placement(
        AxisPlacement::Start(Extent::Content),
        AxisPlacement::Start(Extent::Content),
    );
    let protocol = ProtocolLayout::new(corner.current_layout(), context)?;
    assert_eq!(
        (
            protocol.anchor,
            protocol.width,
            protocol.height,
            protocol.zone
        ),
        (Anchor::Top | Anchor::Left, 500, 62, 0)
    );
    Ok(())
}
