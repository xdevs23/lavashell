// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what its host system does and executes the
//! requests made by the runtime.
#![forbid(unsafe_code)]

mod clipboard;
mod input;
mod output;
mod request;
mod settings;
mod surface;
mod watch;
mod window_control;

pub use clipboard::{
    ClipboardEvent, ClipboardRequest, FormatId, ReceiveId, ReceiveRefusal, SelectionId,
    SelectionRefusal,
};
pub use input::{
    Button, DeviceEvent, InputRequest, InputSerial, KeyboardDevice, PointerDevice, PressState,
};
pub use output::{OutputEvent, OutputId};
pub use request::PlatformRequest;
pub use settings::SettingsEvent;
pub use surface::{Appearance, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec};
pub use watch::{Readiness, WatchEvent, WatchId, WatchRequest};
pub use window_control::WindowControlRequest;
