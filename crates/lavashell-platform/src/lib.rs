// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod accessibility;
mod capability;
mod clipboard;
mod drag;
mod event;
mod focus_grab;
mod input;
mod output;
mod panel;
mod platform;
mod request;
mod session_lock;
mod settings;
mod streams;
mod surface;
#[cfg(test)]
mod test_merge;
mod timing;
mod watch;
mod window_control;

pub use accessibility::{AccessibilityEvent, AccessibilityRequest, tree_difference};
pub use capability::Capability;
pub use clipboard::{
    ClipboardEvent, ClipboardRequest, FormatId, ReceiveId, ReceiveRefusal, SelectionId,
    SelectionRefusal,
};
pub use drag::{
    DragDevice, DragReceiveRefusal, DragRefusal, DragSourceEvent, DragSourceId, DragSourceRequest,
    DragTargetEvent, DragTargetRequest,
};
pub use event::PlatformEvent;
pub use focus_grab::{FocusGrabEvent, FocusGrabRequest};
pub use input::{
    Button, DeviceEvent, InputRequest, InputSerial, KeyAction, KeyboardDevice, PointerDevice,
    PointerPress, PressState,
};
pub use output::{OutputEvent, OutputId};
pub use panel::{KeyboardMode, PanelRequest};
pub use platform::{Connection, HostFacts, Platform, PlatformLinks, Reports, SurfaceAttachment};
pub use request::PlatformRequest;
pub use session_lock::{SessionLockEvent, SessionLockRequest};
pub use settings::SettingsEvent;
pub use streams::{StreamSetup, StreamTask, no_streams};
pub use surface::{
    PanelRole, Role, SurfaceAppearance, SurfaceEvent, SurfaceRequest, SurfaceSpec, WindowRole,
};
pub use timing::TimingEvent;
pub use watch::{Readiness, WatchEvent, WatchId, WatchRequest};
pub use window_control::WindowControlRequest;
