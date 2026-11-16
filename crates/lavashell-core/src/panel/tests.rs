// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::TestResult;
use crate::units::DpExt;

fn layout(horizontal: AxisPlacement, vertical: AxisPlacement) -> LayerLayout {
    LayerLayout {
        margins: LayerMargins::default(),
        horizontal,
        vertical,
        exclusive_zone: ExclusiveZone::LayerExtent,
    }
}

fn content(width: Dp, height: Dp) -> Result<ContentSize, NumberError> {
    ContentSize::try_from(Size::new(width, height))
}

fn length(length: Dp) -> Result<NonNegativeDp, NumberError> {
    NonNegativeDp::try_new(length)
}

#[test]
fn a_layer_length_that_is_not_finite_is_rejected() {
    assert_eq!(Extent::dp(Dp::new(f64::NAN)), Err(NumberError::NotFinite));
    assert_eq!(
        ExclusiveZone::reserve(Dp::new(f64::INFINITY)),
        Err(NumberError::NotFinite)
    );
    assert_eq!(
        content(Dp::new(f64::NAN), 1.dp()),
        Err(NumberError::NotFinite)
    );
}

#[test]
fn a_negative_layer_length_is_rejected() {
    assert_eq!(Extent::dp(-4.dp()), Err(NumberError::Negative));
    assert_eq!(ExclusiveZone::reserve(-4.dp()), Err(NumberError::Negative));
    assert_eq!(content(1.dp(), -4.dp()), Err(NumberError::Negative));
}

#[test]
fn a_bar_at_the_top_takes_the_measured_height_of_its_content_in_whole_dp() -> TestResult {
    let bar = PanelGeometry::of(
        layout(AxisPlacement::Fill, AxisPlacement::Start(Extent::Content)),
        content(500.dp(), 61.6.dp())?,
    );
    assert_eq!(bar.horizontal(), AxisPlacement::Fill);
    assert_eq!(bar.vertical(), AxisPlacement::Start(length(62.dp())?));
    assert_eq!(bar.exclusive_zone(), ExclusiveZone::LayerExtent);
    Ok(())
}

#[test]
fn a_panel_shorter_than_half_a_dp_is_one_dp_long() -> TestResult {
    let centered = AxisPlacement::Center(Extent::Content);
    let thin = PanelGeometry::of(
        layout(centered, AxisPlacement::Start(Extent::Content)),
        content(0.3.dp(), Dp::ZERO)?,
    );
    assert_eq!(thin.horizontal(), AxisPlacement::Center(length(1.dp())?));
    assert_eq!(thin.vertical(), AxisPlacement::Start(length(1.dp())?));
    Ok(())
}

#[test]
fn fixed_lengths_margins_and_a_zone_ignore_the_content_and_round_to_whole_dp() -> TestResult {
    let popover = LayerLayout {
        margins: LayerMargins {
            top: FiniteDp::try_new(48.dp())?,
            left: FiniteDp::try_new(-8.4.dp())?,
            ..LayerMargins::default()
        },
        horizontal: AxisPlacement::Start(Extent::dp(320.6.dp())?),
        vertical: AxisPlacement::End(Extent::dp(344.dp())?),
        exclusive_zone: ExclusiveZone::reserve(12.4.dp())?,
    };
    let geometry = PanelGeometry::of(popover, content(500.dp(), 61.6.dp())?);
    assert_eq!(
        geometry.horizontal(),
        AxisPlacement::Start(length(321.dp())?)
    );
    assert_eq!(geometry.vertical(), AxisPlacement::End(length(344.dp())?));
    assert_eq!(
        geometry.exclusive_zone(),
        ExclusiveZone::Reserve(length(12.dp())?)
    );
    let margins = geometry.margins();
    assert_eq!((margins.top.get(), margins.left.get()), (48.dp(), -8.dp()));
    Ok(())
}

#[test]
fn only_fixed_lengths_bound_the_measured_content() -> TestResult {
    let fixed = AxisPlacement::Start(Extent::dp(320.dp())?);
    let measured = layout(fixed, AxisPlacement::Start(Extent::Content));
    assert!(measured.follows_content());
    assert_eq!(measured.fixed_size(), None);
    let proposal = measured.content_proposal();
    assert_eq!((proposal.width, proposal.height), (Some(320.dp()), None));
    let popover = layout(fixed, fixed);
    assert!(!popover.follows_content());
    assert_eq!(popover.fixed_size(), Some(Size::new(320.dp(), 320.dp())));
    Ok(())
}
