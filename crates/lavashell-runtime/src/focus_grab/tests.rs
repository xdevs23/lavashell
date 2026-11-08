// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use crate::{KeyboardPolicy, PanelContent, PanelOptions};
use lavashell_core::preferences::PreferenceOverrides;
use lavashell_core::prelude::Modifier;
use lavashell_core::reactive::State;
use lavashell_core::units::{Density, DpExt};
use lavashell_core::view::View;
use lavashell_platform::{SurfaceEvent, SurfaceRequest};
use lavashell_test_fixtures::NoText;
use std::cell::Cell;
use std::rc::Rc;

fn surface_size() -> Size {
    Size::new(500.dp(), 40.dp())
}

fn other_size() -> Size {
    Size::new(500.dp(), 41.dp())
}

/// A configured surface that shows a panel allowed to receive the keyboard focus, with a focus grab on the panel.
struct Host {
    panels: PanelRecords,
    surfaces: SurfaceRecords,
    grabs: FocusGrabs,
    surface: SurfaceId,
}

impl Host {
    fn new(grab: FocusGrab) -> Result<Self, TestError> {
        let mut view = View::new(lavashell_test_fixtures::facts(), |ui| {
            ui.spacer(500.dp(), 40.dp(), Modifier::new());
        });
        let options = PanelOptions {
            keyboard: KeyboardPolicy::OnDemand,
            ..PanelOptions::default()
        };
        let panel = Panel::new(options, &mut view, &mut NoText)?;
        let mut surfaces = SurfaceRecords::new(PreferenceOverrides::default());
        let surface = surfaces.open(view, surface_size(), Density::ONE, None, String::new())?;
        let configure = SurfaceEvent::Configured {
            width: None,
            height: None,
        };
        assert_eq!(surfaces.handle(surface, configure), None);
        let mut panels = PanelRecords::default();
        panels.open(surface, panel);
        let mut grabs = FocusGrabs::default();
        grabs.open(surface, grab);
        Ok(Self {
            panels,
            surfaces,
            grabs,
            surface,
        })
    }

    /// Reports that the host shows a frame with `size`, which accepts input or not.
    fn show(&mut self, input: bool, size: Size) {
        let content = PanelContent { size, input };
        self.panels.shown(self.surface, content);
    }

    fn update(&mut self) -> Vec<FocusGrabRequest> {
        self.grabs.update(&self.panels, &self.surfaces)
    }

    fn clear(&mut self) {
        self.grabs.handle(FocusGrabEvent::Cleared(self.surface));
    }

    fn acquire(&self) -> FocusGrabRequest {
        FocusGrabRequest::Acquire(self.surface)
    }

    fn release(&self) -> FocusGrabRequest {
        FocusGrabRequest::Release(self.surface)
    }
}

/// A grab that is active while `active` is true.
fn grab(active: &State<bool>) -> FocusGrab {
    let active = active.clone();
    FocusGrab::new(move || active.get(), || {})
}

#[test]
fn a_grab_starts_once_the_shown_content_accepts_input_and_fits_the_surface() -> TestResult {
    let mut host = Host::new(grab(&State::new(true)))?;
    assert!(host.update().is_empty());
    host.show(false, surface_size());
    assert!(host.update().is_empty());
    host.show(true, other_size());
    assert!(host.update().is_empty());
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    assert!(host.update().is_empty());
    Ok(())
}

#[test]
fn a_started_grab_ends_with_the_input_of_the_content_or_the_activity_of_the_panel() -> TestResult {
    let active = State::new(true);
    let mut host = Host::new(grab(&active))?;
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    host.show(true, other_size());
    assert!(host.update().is_empty());
    host.show(false, surface_size());
    assert_eq!(host.update(), [host.release()]);
    assert!(host.update().is_empty());
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    active.set(false);
    assert_eq!(host.update(), [host.release()]);
    assert!(host.update().is_empty());
    Ok(())
}

#[test]
fn a_cleared_grab_of_an_active_panel_waits_for_the_panel_to_become_inactive() -> TestResult {
    let active = State::new(true);
    let mut host = Host::new(grab(&active))?;
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    host.clear();
    assert!(host.update().is_empty());
    host.show(true, surface_size());
    assert!(host.update().is_empty());
    active.set(false);
    assert!(host.update().is_empty());
    active.set(true);
    assert_eq!(host.update(), [host.acquire()]);
    Ok(())
}

#[test]
fn the_cleared_callback_can_make_the_panel_inactive_before_the_state_is_read() -> TestResult {
    let active = State::new(true);
    let closed = active.clone();
    let observed = active.clone();
    let grab = FocusGrab::new(move || observed.get(), move || closed.set(false));
    let mut host = Host::new(grab)?;
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    host.clear();
    assert!(!active.get());
    active.set(true);
    assert_eq!(host.update(), [host.acquire()]);
    Ok(())
}

#[test]
fn a_clear_without_a_started_grab_does_not_run_the_cleared_callback() -> TestResult {
    let cleared = Rc::new(Cell::new(0_u32));
    let counted = cleared.clone();
    let grab = FocusGrab::new(|| true, move || counted.set(counted.get() + 1));
    let mut host = Host::new(grab)?;
    host.clear();
    assert_eq!(cleared.get(), 0);
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    host.clear();
    host.clear();
    assert_eq!(cleared.get(), 1);
    Ok(())
}

#[test]
fn a_grab_makes_no_request_once_its_surface_closed() -> TestResult {
    let mut host = Host::new(grab(&State::new(true)))?;
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    let closed = host.surfaces.handle(host.surface, SurfaceEvent::Closed);
    assert_eq!(closed, Some(SurfaceRequest::Destroy(host.surface)));
    host.show(false, surface_size());
    assert!(host.update().is_empty());
    Ok(())
}

#[test]
fn a_forgotten_grab_requests_its_release_only_when_it_was_started() -> TestResult {
    let cleared = Rc::new(Cell::new(0_u32));
    let counted = cleared.clone();
    let grab = FocusGrab::new(|| true, move || counted.set(counted.get() + 1));
    let mut host = Host::new(grab.clone())?;
    assert_eq!(host.grabs.forget(host.surface), None);
    host.grabs.open(host.surface, grab);
    host.show(true, surface_size());
    assert_eq!(host.update(), [host.acquire()]);
    assert_eq!(host.grabs.forget(host.surface), Some(host.release()));
    host.clear();
    assert_eq!(cleared.get(), 0);
    host.show(false, surface_size());
    assert!(host.update().is_empty());
    Ok(())
}
