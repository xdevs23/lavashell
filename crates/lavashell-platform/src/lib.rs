// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what its host system does and executes the
//! requests made by the runtime.
#![forbid(unsafe_code)]

mod accessibility;
mod clipboard;
mod drag;
mod focus_grab;
mod input;
mod lock;
mod output;
mod panel;
mod request;
mod settings;
mod surface;
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
pub use focus_grab::{FocusGrabEvent, FocusGrabRequest};
pub use input::{
    Button, DeviceEvent, InputRequest, InputSerial, KeyboardDevice, PointerDevice, PressState,
};
pub use lock::LockEvent;
pub use output::{OutputEvent, OutputId};
pub use panel::{KeyboardMode, PanelRequest};
pub use request::PlatformRequest;
pub use settings::SettingsEvent;
pub use surface::{Appearance, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec};
pub use timing::TimingEvent;
pub use watch::{Readiness, WatchEvent, WatchId, WatchRequest};
pub use window_control::WindowControlRequest;
