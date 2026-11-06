// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::geometry::Size;
use crate::panel::{AxisPlacement, ContentSize, ExclusiveZone, LayerLayout, LayerMargins};
use crate::test_error::TestResult;
use crate::units::DpExt;

fn panel() -> Result<PanelGeometry, crate::units::NumberError> {
    let layout = LayerLayout {
        margins: LayerMargins::default(),
        horizontal: AxisPlacement::Fill,
        vertical: AxisPlacement::Fill,
        exclusive_zone: ExclusiveZone::IgnoreOthers,
    };
    Ok(PanelGeometry::of(
        layout,
        ContentSize::try_from(Size::ZERO)?,
    ))
}

#[test]
fn a_state_contains_a_change_only_once_the_change_is_applied() -> TestResult {
    let mut state = SurfaceState::default();
    let region = SurfaceChange::InputRegion(Some(vec![Rect::new(0.dp(), 0.dp(), 4.dp(), 2.dp())]));
    assert!(!state.contains(&region));
    state.apply(region.clone());
    assert!(state.contains(&region));
    assert!(!state.contains(&SurfaceChange::InputRegion(None)));
    let geometry = SurfaceChange::Panel(panel()?);
    assert!(state.get(SurfaceField::Panel).is_none());
    state.apply(geometry.clone());
    assert_eq!(state.get(SurfaceField::Panel), Some(&geometry));
    Ok(())
}

#[test]
fn the_changes_of_a_state_turn_the_default_state_into_it() -> TestResult {
    let mut state = SurfaceState::default();
    state.apply(SurfaceChange::BlurRegion(Some(Vec::new())));
    state.apply(SurfaceChange::Panel(panel()?));
    state.apply(SurfaceChange::Configure(ConfigureToken::default()));
    state.apply(SurfaceChange::BlurRegion(None));
    let mut other = SurfaceState::default();
    for change in state.changes() {
        other.apply(change);
    }
    assert_eq!(other, state);
    assert_eq!(state.changes().count(), 3);
    Ok(())
}

#[test]
fn every_configure_token_is_new() {
    assert_ne!(ConfigureToken::default(), ConfigureToken::default());
}
