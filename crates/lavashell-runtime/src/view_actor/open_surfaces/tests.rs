// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::panels::{KeyboardPolicy, PanelOptions};
use crate::test_error::TestResult;
use crate::test_host::empty_view;
use lavashell_core::units::DpExt;
use lavashell_test_fixtures::NoText;

#[test]
fn a_state_commit_before_any_composed_frame_applies_the_panel_requests_and_leaves_input_off()
-> TestResult {
    let mut view = empty_view();
    let options = PanelOptions {
        keyboard: KeyboardPolicy::OnDemand,
        ..PanelOptions::default()
    };
    let panel = Panel::new(options, &mut view, &mut NoText)?;
    let mut surfaces = OpenSurfaces::new(PreferenceOverrides::default(), &BTreeSet::new());
    let opening = Opening {
        shape: None,
        accessible_name: None,
        composition_order: 0,
    };
    let id = surfaces.open(view, Size::new(500.dp(), 40.dp()), opening)?;
    surfaces.panels.open(id, panel);
    let mut state = SurfaceState::default();
    if let Some(geometry) = surfaces.panel_geometry_change(id) {
        state.apply(geometry);
    }
    let requests = surfaces.panel_requests(id, &state);
    if let Some(surface) = surfaces.view_surfaces.get_mut(&id) {
        surface.start_commit(requests);
    }
    assert!(surfaces.committed(id).is_none());
    let panel = surfaces.panels.get(id);
    assert!(panel.is_some_and(|panel| panel.keyboard_granted() && !panel.input_enabled()));
    Ok(())
}
