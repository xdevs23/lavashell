// SPDX-License-Identifier: GPL-3.0-only
//! The messages between the runtime and a platform. A platform reports what the host system does with each surface.
//! It executes the requests made by the runtime.
#![forbid(unsafe_code)]

mod surface;

pub use surface::{SurfaceEvent, SurfaceRequest};
