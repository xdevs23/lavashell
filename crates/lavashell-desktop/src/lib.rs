// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod layer;
mod platform;
mod services;

#[cfg(any(test, feature = "test-support"))]
pub use layer::build_services;
pub use layer::{plug_in, register_in_layers};
pub use platform::{DesktopPlatform, DesktopPlatformError};
pub use services::{DesktopService, ServiceWatch, ServicesStartError};
