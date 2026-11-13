// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_merge::merged;
use crate::{KeyboardMode, WatchId};
use accesskit::{NodeId, TreeId, TreeUpdate};
use lavashell_actor::Closed;
use lavashell_core::view::Cursor;

const MOUSE: PointerDevice = PointerDevice(1);
const PEN: PointerDevice = PointerDevice(2);
const SURFACE: SurfaceId = SurfaceId(1);
const OTHER: SurfaceId = SurfaceId(2);

fn set_cursor(pointer: PointerDevice, cursor: Cursor) -> PlatformRequest {
    PlatformRequest::Input(InputRequest::SetCursor(pointer, cursor))
}

/// The pointer and the cursor of a cursor request, or `None` for every other request.
fn cursor_of(request: &PlatformRequest) -> Option<(PointerDevice, Cursor)> {
    match request {
        PlatformRequest::Input(InputRequest::SetCursor(pointer, cursor)) => {
            Some((*pointer, *cursor))
        }
        _ => None,
    }
}

fn set_keyboard(surface: SurfaceId, mode: KeyboardMode) -> PlatformRequest {
    PlatformRequest::Panel(PanelRequest::SetKeyboard(surface, mode))
}

/// The panel request, or `None` for every other request.
fn panel_request(request: &PlatformRequest) -> Option<PanelRequest> {
    match request {
        PlatformRequest::Panel(request) => Some(*request),
        _ => None,
    }
}

fn publish(surface: SurfaceId, focus: u64) -> PlatformRequest {
    let tree = TreeUpdate {
        nodes: Vec::new(),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: NodeId(focus),
    };
    PlatformRequest::Accessibility(AccessibilityRequest::Publish {
        surface,
        tree: Box::new(tree),
    })
}

fn set_window_focus(surface: SurfaceId, focused: bool) -> PlatformRequest {
    PlatformRequest::Accessibility(AccessibilityRequest::SetWindowFocus { surface, focused })
}

/// The accessibility request, or `None` for every other request.
fn accessibility(request: &PlatformRequest) -> Option<&AccessibilityRequest> {
    match request {
        PlatformRequest::Accessibility(request) => Some(request),
        _ => None,
    }
}

/// The surface and the focused node of a published tree, or `None` for every other request.
fn published_focus(request: &PlatformRequest) -> Option<(SurfaceId, NodeId)> {
    match accessibility(request)? {
        AccessibilityRequest::Publish { surface, tree } => Some((*surface, tree.focus)),
        AccessibilityRequest::SetWindowFocus { .. } => None,
    }
}

/// The surface and the focus of a window focus request, or `None` for every other request.
fn window_focus(request: &PlatformRequest) -> Option<(SurfaceId, bool)> {
    match accessibility(request)? {
        AccessibilityRequest::SetWindowFocus { surface, focused } => Some((*surface, *focused)),
        AccessibilityRequest::Publish { .. } => None,
    }
}

#[test]
fn a_cursor_replaces_the_waiting_cursor_of_its_pointer_only() -> Result<(), Closed> {
    let requests = [
        set_cursor(MOUSE, Cursor::Default),
        set_cursor(PEN, Cursor::Pointer),
        set_cursor(MOUSE, Cursor::Text),
    ];
    let taken: Vec<_> = merged(requests)?.iter().map(cursor_of).collect();
    let expected = [Some((MOUSE, Cursor::Text)), Some((PEN, Cursor::Pointer))];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_cursor_after_another_request_keeps_its_place() -> Result<(), Closed> {
    let requests = [
        set_cursor(MOUSE, Cursor::Text),
        PlatformRequest::Lock(LockRequest::Lock),
        set_cursor(MOUSE, Cursor::Default),
    ];
    let taken: Vec<_> = merged(requests)?.iter().map(cursor_of).collect();
    let expected = [
        Some((MOUSE, Cursor::Text)),
        None,
        Some((MOUSE, Cursor::Default)),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_panel_keyboard_mode_replaces_the_waiting_mode_of_its_surface_only() -> Result<(), Closed> {
    let requests = [
        set_keyboard(SURFACE, KeyboardMode::OnDemand),
        set_keyboard(OTHER, KeyboardMode::OnDemand),
        set_keyboard(SURFACE, KeyboardMode::Off),
    ];
    let taken: Vec<_> = merged(requests)?.iter().map(panel_request).collect();
    let expected = [
        Some(PanelRequest::SetKeyboard(SURFACE, KeyboardMode::Off)),
        Some(PanelRequest::SetKeyboard(OTHER, KeyboardMode::OnDemand)),
    ];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn an_accessibility_tree_replaces_the_waiting_tree_of_its_surface_only() -> Result<(), Closed> {
    let requests = [
        publish(SURFACE, 10),
        publish(OTHER, 20),
        publish(SURFACE, 11),
    ];
    let taken: Vec<_> = merged(requests)?.iter().map(published_focus).collect();
    let expected = [Some((SURFACE, NodeId(11))), Some((OTHER, NodeId(20)))];
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn a_window_focus_replaces_the_waiting_focus_of_its_surface_only() -> Result<(), Closed> {
    let requests = [
        set_window_focus(SURFACE, true),
        set_window_focus(OTHER, true),
        set_window_focus(SURFACE, false),
    ];
    let taken: Vec<_> = merged(requests)?.iter().map(window_focus).collect();
    assert_eq!(taken, [Some((SURFACE, false)), Some((OTHER, true))]);
    Ok(())
}

#[test]
fn every_other_request_builds_on_in_order() -> Result<(), Closed> {
    let requests = [
        PlatformRequest::Lock(LockRequest::Lock),
        PlatformRequest::Watch(WatchRequest::Resume(WatchId(1))),
        PlatformRequest::Watch(WatchRequest::Resume(WatchId(1))),
        PlatformRequest::Surface(SurfaceRequest::Destroy(SURFACE)),
        PlatformRequest::Lock(LockRequest::Unlock),
    ];
    let taken = merged(requests)?;
    assert!(
        matches!(
            taken.as_slice(),
            [
                PlatformRequest::Lock(LockRequest::Lock),
                PlatformRequest::Watch(WatchRequest::Resume(WatchId(1))),
                PlatformRequest::Watch(WatchRequest::Resume(WatchId(1))),
                PlatformRequest::Surface(SurfaceRequest::Destroy(SURFACE)),
                PlatformRequest::Lock(LockRequest::Unlock),
            ]
        ),
        "{taken:?}"
    );
    Ok(())
}
