// SPDX-License-Identifier: GPL-3.0-only
use crate::surface_records::SurfaceRecords;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{InputSerial, PointerDevice, WindowControlRequest};

/// The request that starts a move of the window `surface` by the host, when the press of the primary button of
/// `pointer` at `at` lies in the drag region of the window.
pub(crate) fn begin_move(
    records: &SurfaceRecords,
    surface: SurfaceId,
    pointer: PointerDevice,
    at: Point,
    serial: InputSerial,
) -> Option<WindowControlRequest> {
    (records.in_drag_region(surface, at)).then_some(WindowControlRequest::BeginMove {
        surface,
        pointer,
        serial,
    })
}
