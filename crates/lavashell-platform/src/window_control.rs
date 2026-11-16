// SPDX-License-Identifier: GPL-3.0-only
use crate::{InputSerial, PointerDevice};
use lavashell_core::surface::SurfaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowControlRequest {
    BeginMove {
        surface: SurfaceId,
        pointer: PointerDevice,
        serial: InputSerial,
    },
}
