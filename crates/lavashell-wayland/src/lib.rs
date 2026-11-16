// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod bind;
mod clipboard;
mod error;
mod focus_grab;
mod logical;
mod native;
mod preferences;
mod protocol_serial;
#[cfg(test)]
mod test_bus;
#[cfg(test)]
mod test_error;
mod translator;
mod wayland_presentation;

use smithay_client_toolkit as sctk;

pub use clipboard::DragSourceError;
pub use error::Error;
pub use native::control::ControlError;
pub use native::device::DeviceError;
pub use native::focus_grab::FocusGrabError;
pub use native::layer::{LayerError, LayerLayoutError};
pub use native::preferences::PreferencesError;
pub use native::session_lock::SessionLockError;
pub use native::startup::WindowError;
pub use native::watch::WatchError;
pub use native::window_input::WindowControlError;
pub use translator::ConnectionError;
pub use translator::EventLoopError;
pub use translator::Wayland;
pub use translator::requests::RequestsError;
