// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what the host system does with each surface
//! and each output. It executes the requests made by the runtime.
#![forbid(unsafe_code)]

mod output;
mod surface;

pub use output::{OutputEvent, OutputId};
pub use surface::{Appearance, Role, SurfaceEvent, SurfaceRequest, SurfaceSpec};
