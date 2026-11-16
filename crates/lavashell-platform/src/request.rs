// SPDX-License-Identifier: GPL-3.0-only
use crate::accessibility::AccessibilityRequestKey;
use crate::{
    AccessibilityRequest, ClipboardRequest, DragSourceRequest, DragTargetRequest, FocusGrabRequest,
    InputRequest, PanelRequest, PointerDevice, SessionLockRequest, SurfaceRequest, WatchRequest,
    WindowControlRequest,
};
use lavashell_actor::{Merge, MergeEffect};
use lavashell_core::surface::SurfaceId;

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
    Lock(SessionLockRequest),
    FocusGrab(FocusGrabRequest),
    Accessibility(AccessibilityRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestKey {
    Cursor(PointerDevice),
    PanelKeyboard(SurfaceId),
    Accessibility(AccessibilityRequestKey),
}

impl Merge for PlatformRequest {
    type Key = RequestKey;

    fn effect(&self) -> MergeEffect<RequestKey> {
        match self {
            Self::Input(request) => request.effect().map(RequestKey::Cursor),
            Self::Panel(request) => request.effect().map(RequestKey::PanelKeyboard),
            Self::Accessibility(request) => request.effect().map(RequestKey::Accessibility),
            Self::Surface(_)
            | Self::WindowControl(_)
            | Self::Watch(_)
            | Self::Clipboard(_)
            | Self::DragTarget(_)
            | Self::DragSource(_)
            | Self::Lock(_)
            | Self::FocusGrab(_) => MergeEffect::Appends,
        }
    }
}

#[cfg(test)]
mod tests;
