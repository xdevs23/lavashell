// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what its host system does and executes the
//! requests made by the runtime.
#![forbid(unsafe_code)]

mod input;
mod output;
mod panel;
mod settings;
mod surface;
mod timing;
mod watch;

pub use input::{Button, DeviceEvent, InputRequest, KeyboardDevice, PointerDevice, PressState};
pub use output::{OutputEvent, OutputId};
pub use panel::KeyboardMode;
pub use settings::SettingsEvent;
pub use surface::{Appearance, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec};
pub use timing::TimingEvent;
pub use watch::{WatchEvent, WatchId, WatchRequest};
