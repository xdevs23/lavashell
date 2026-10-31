// SPDX-License-Identifier: GPL-3.0-only
mod accessibility;
mod app;
mod clipboard;
mod error;
mod failures;
mod focus_grab;
mod logical;
#[cfg(test)]
mod preference_reader;
mod preferences;
mod shape;
mod single_precision;
mod surface;
#[cfg(test)]
mod test_bus;
#[cfg(test)]
mod test_error;
mod text_transfer;

use smithay_client_toolkit as sctk;

pub use app::output_layers::MonitorInfo;
pub use app::session_lock::SessionLock;
pub use app::{App, HostKey, KeyboardFocus, ViewActions};
pub use clipboard::{
    Clipboard, DragEvent, DragGesture, DragOfferInfo, DragRequest, DropEvent, TransferId,
};
pub use error::{Cancellation, Error};
pub use focus_grab::FocusGrab;
pub use shape::WindowShape;
pub use surface::{
    AxisPlacement, ExclusiveZone, Extent, LayerLayout, LayerLevel, LayerMargins, LayerSurface,
    Window,
};
