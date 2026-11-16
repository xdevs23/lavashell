// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::layer_surface::LayerSurface;
use crate::run::SurfaceSetup;
use crate::session_lock::{LockConfiguration, SessionLock};
use crate::test_host::{empty_view, runtime_setup, window};
use crate::window::Window;
use lavashell_core::focus_grab::FocusGrab;

fn offering(capabilities: impl IntoIterator<Item = Capability>) -> BTreeSet<Capability> {
    capabilities.into_iter().collect()
}

fn panel(layer: LayerSurface) -> SurfaceSetup {
    SurfaceSetup::Panel(layer, empty_view())
}

fn locking(mut setup: RuntimeSetup) -> RuntimeSetup {
    let platform = lavashell_test_fixtures::facts();
    setup.session_lock = Some(LockConfiguration::new(SessionLock::new(), platform, |_| {}));
    setup
}

#[test]
fn a_setup_without_optional_subsystems_needs_no_capability() {
    let setup = runtime_setup(vec![window("plain")]);
    assert!(missing(&setup, &offering([])).is_empty());
}

#[test]
fn each_used_subsystem_needs_its_capability() {
    let movable = Window::new("movable", "test.app").drag_region(|_, _| true);
    let grabbing = LayerSurface::new("popover").focus_grab(FocusGrab::new(|| true, || {}));
    let mut dragging = runtime_setup(vec![window("target")]);
    dragging.drags.on_drop(|_, _| true);
    let copying = runtime_setup(vec![window("editor")]);
    let _handle = copying.clipboard.clipboard();
    let setups = [
        (
            runtime_setup(vec![panel(LayerSurface::new("bar"))]),
            offering([Capability::Panels]),
        ),
        (
            locking(runtime_setup(Vec::new())),
            offering([Capability::SessionLock]),
        ),
        (
            runtime_setup(vec![panel(grabbing)]),
            offering([Capability::Panels, Capability::FocusGrab]),
        ),
        (
            runtime_setup(vec![SurfaceSetup::Window(movable, empty_view())]),
            offering([Capability::WindowMove]),
        ),
        (copying, offering([Capability::Clipboard])),
        (dragging, offering([Capability::DragAndDrop])),
    ];
    for (setup, used) in setups {
        assert_eq!(missing(&setup, &offering([])), used);
        assert!(missing(&setup, &used).is_empty());
    }
}

#[test]
fn the_built_in_text_drag_needs_no_capability() {
    let mut setup = runtime_setup(vec![window("text")]);
    setup.drags.enable_text_transfers();
    assert!(missing(&setup, &offering([])).is_empty());
}

#[test]
fn every_missing_capability_is_contained() {
    let setup = locking(runtime_setup(vec![panel(LayerSurface::new("bar"))]));
    assert_eq!(
        missing(&setup, &offering([])),
        offering([Capability::Panels, Capability::SessionLock])
    );
    assert_eq!(
        missing(&setup, &offering([Capability::Panels])),
        offering([Capability::SessionLock])
    );
}
