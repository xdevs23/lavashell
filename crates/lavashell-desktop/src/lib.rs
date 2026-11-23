// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod layer;
mod services;

#[cfg(any(test, feature = "test-support"))]
pub use layer::build_services;
pub use layer::{plug_in, register_in_layers};
pub use services::{DesktopService, DesktopServices, PanicValue, ServiceWatch, ServicesStartError};
