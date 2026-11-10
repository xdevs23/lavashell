// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn all_contains_every_capability_once() {
    let [panels, lock, grab, moves, clipboard, drags, accessibility] = Capability::ALL;
    // The match is exhaustive, so a new variant needs an arm, and with it a place in `ALL`.
    let listed = |capability| match capability {
        Capability::Panels => panels,
        Capability::SessionLock => lock,
        Capability::FocusGrab => grab,
        Capability::WindowMove => moves,
        Capability::Clipboard => clipboard,
        Capability::DragAndDrop => drags,
        Capability::Accessibility => accessibility,
    };
    assert!(Capability::ALL.is_sorted_by(|first, second| first < second));
    for capability in Capability::ALL {
        assert_eq!(listed(capability), capability);
    }
}
