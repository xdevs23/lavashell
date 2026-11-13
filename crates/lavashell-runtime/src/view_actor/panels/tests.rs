// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use crate::{LayerSurface, Panel};
use lavashell_core::panel::{ContentSize, ExclusiveZone, Extent, LayerLayout, LayerMargins};
use lavashell_core::reactive::State;
use lavashell_core::units::DpExt;
use lavashell_core::view::View;
use lavashell_test_fixtures::NoText;

fn layout(horizontal: AxisPlacement, vertical: AxisPlacement) -> LayerLayout {
    LayerLayout {
        horizontal,
        vertical,
        exclusive_zone: ExclusiveZone::IgnoreOthers,
        margins: LayerMargins::default(),
    }
}

/// The geometry of `layout` for content without a size.
fn geometry(layout: LayerLayout) -> Result<PanelGeometry, TestError> {
    let content = ContentSize::try_from(Size::new(Dp::ZERO, Dp::ZERO))?;
    Ok(PanelGeometry::of(layout, content))
}

#[test]
fn a_panel_opens_at_the_size_of_its_layout_state_over_the_static_placement() -> TestResult {
    let state = layout(
        AxisPlacement::Start(Extent::dp(321.dp())?),
        AxisPlacement::Start(Extent::dp(345.dp())?),
    );
    let short = AxisPlacement::Start(Extent::dp(10.dp())?);
    let options = LayerSurface::new("stateful")
        .placement(AxisPlacement::Fill, short)
        .layout_state(State::new(state));
    let mut view = View::new(lavashell_test_fixtures::facts(), |_| {});
    let panel = Panel::new(options.panel, &mut view, &mut NoText)?;
    assert_eq!(
        opening_size(panel.geometry()),
        Size::new(321.dp(), 345.dp())
    );
    Ok(())
}

#[test]
fn an_axis_that_fills_opens_at_one_logical_pixel() -> TestResult {
    let bar = layout(
        AxisPlacement::Fill,
        AxisPlacement::Start(Extent::dp(30.dp())?),
    );
    assert_eq!(
        opening_size(geometry(bar)?),
        Size::new(OPENING_LENGTH, 30.dp())
    );
    Ok(())
}

#[test]
fn an_axis_left_to_the_panel_by_a_configure_takes_the_length_requested_along_it() -> TestResult {
    let requested = geometry(layout(
        AxisPlacement::Fill,
        AxisPlacement::Start(Extent::dp(400.dp())?),
    ))?;
    let vertical = requested.vertical();
    let horizontal = requested.horizontal();
    assert_eq!(configured_length(None, vertical), Some(400.dp()));
    assert_eq!(configured_length(Some(380.dp()), vertical), Some(380.dp()));
    assert_eq!(
        configured_length(Some(1920.dp()), horizontal),
        Some(1920.dp())
    );
    assert_eq!(configured_length(None, horizontal), None);
    Ok(())
}
