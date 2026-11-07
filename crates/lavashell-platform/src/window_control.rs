// SPDX-License-Identifier: GPL-3.0-only
use crate::{InputSerial, PointerDevice};
use lavashell_core::surface::SurfaceId;

/// What the runtime asks a platform to do with a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControlRequest {
    /// Starts a move of the window `surface` by the host. The window follows `pointer` from the press with `serial`
    /// until the button is released.
    BeginMove {
        surface: SurfaceId,
        pointer: PointerDevice,
        serial: InputSerial,
    },
}
