// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what the host system does with each surface
//! and each output, and what the pointers and keyboards connected to the host do. It executes the requests made by
//! the runtime.
#![forbid(unsafe_code)]

mod input;
mod output;
mod surface;

pub use input::{Button, DeviceEvent, InputRequest, KeyboardDevice, PointerDevice, PressState};
pub use output::{OutputEvent, OutputId};
pub use surface::{Appearance, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec};
