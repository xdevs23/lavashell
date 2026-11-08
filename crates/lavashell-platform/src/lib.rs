// SPDX-License-Identifier: GPL-3.0-only
//! A platform and the messages between it and the runtime. A platform runs on its own thread, reports what the host
//! system does and executes the requests made by the runtime.
#![forbid(unsafe_code)]

mod accessibility;
mod clipboard;
mod drag;
mod event;
mod focus_grab;
mod input;
mod lock;
mod output;
mod panel;
mod platform;
mod request;
mod settings;
mod surface;
#[cfg(test)]
mod test_merge;
mod timing;
mod watch;
mod window_control;

pub use accessibility::{AccessibilityEvent, AccessibilityRequest, tree_difference};
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
    PressState,
};
pub use lock::{LockEvent, LockRequest};
pub use output::{OutputEvent, OutputId};
pub use panel::{KeyboardMode, PanelRequest};
pub use platform::{Connection, HostFacts, Platform, PlatformLinks, Reports, SurfaceAttachment};
pub use request::PlatformRequest;
pub use settings::SettingsEvent;
pub use surface::{
    Appearance, PanelRole, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec, WindowRole,
};
pub use timing::TimingEvent;
pub use watch::{Readiness, WatchEvent, WatchId, WatchRequest};
pub use window_control::WindowControlRequest;
