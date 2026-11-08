// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    AccessibilityRequest, ClipboardRequest, DragSourceRequest, DragTargetRequest, FocusGrabRequest,
    InputRequest, LockRequest, PanelRequest, PointerDevice, SurfaceRequest, WatchRequest,
    WindowControlRequest,
};
use lavashell_actor::{Effect, Merge};
use lavashell_core::surface::SurfaceId;

/// What the runtime asks a platform to do.
#[derive(Debug)]
pub enum PlatformRequest {
    Surface(SurfaceRequest),
    Input(InputRequest),
    WindowControl(WindowControlRequest),
    Panel(PanelRequest),
    Watch(WatchRequest),
    Clipboard(ClipboardRequest),
    DragTarget(DragTargetRequest),
    DragSource(DragSourceRequest),
    Lock(LockRequest),
    FocusGrab(FocusGrabRequest),
    Accessibility(AccessibilityRequest),
}

/// A value that a request sets as a whole, so that a later request for the same value replaces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestKey {
    Cursor(PointerDevice),
    PanelKeyboard(SurfaceId),
    AccessibilityTree(SurfaceId),
    WindowFocus(SurfaceId),
}

impl Merge for PlatformRequest {
    type Key = RequestKey;

    /// The cursor of a pointer, the keyboard mode of a panel, and the accessibility tree and the window focus of a
    /// surface replace the earlier request for the same value. Every other request builds on the requests before it.
    fn effect(&self) -> Effect<RequestKey> {
        let key = match *self {
            Self::Input(InputRequest::SetCursor(pointer, _)) => RequestKey::Cursor(pointer),
            Self::Panel(PanelRequest::SetKeyboard(surface, _)) => {
                RequestKey::PanelKeyboard(surface)
            }
            Self::Accessibility(AccessibilityRequest::Publish { surface, .. }) => {
                RequestKey::AccessibilityTree(surface)
            }
            Self::Accessibility(AccessibilityRequest::SetWindowFocus { surface, .. }) => {
                RequestKey::WindowFocus(surface)
            }
            _ => return Effect::BuildsOn,
        };
        Effect::Overwrites(key)
    }
}

#[cfg(test)]
mod tests;
