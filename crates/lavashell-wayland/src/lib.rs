// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod app;
mod clipboard;
mod error;
mod focus_grab;
mod logical;
mod preferences;
#[cfg(test)]
mod test_bus;
#[cfg(test)]
mod test_error;
mod translator;
mod wayland_presentation;

use smithay_client_toolkit as sctk;

pub use app::control::ControlError;
pub use app::device::DeviceError;
pub use app::focus_grab::FocusGrabError;
pub use app::layer::{LayerError, LayerLayoutError};
pub use app::preferences::PreferencesError;
pub use app::session_lock::SessionLockError;
pub use app::startup::WindowError;
pub use app::watch::WatchError;
pub use error::Error;
pub use translator::ConnectionError;
pub use translator::EventLoopError;
pub use translator::Wayland;
pub use translator::requests::RequestsError;
