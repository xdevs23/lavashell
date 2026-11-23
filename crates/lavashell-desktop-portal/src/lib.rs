// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

mod error;
mod settings;
mod settings_watch_failure;
#[cfg(test)]
mod test_error;
#[cfg(any(test, feature = "test-support"))]
pub mod test_portal;

pub use error::PortalError;
pub use settings::{
    SettingChange, SettingChanges, SettingValue, WatchedNamespace, watch_namespace,
};
