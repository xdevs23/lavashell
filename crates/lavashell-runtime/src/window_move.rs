// SPDX-License-Identifier: GPL-3.0-only
use crate::surface_records::SurfaceRecords;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_core::window::WindowRegion;
use lavashell_platform::{PointerPress, WindowControlRequest};

pub(crate) fn begin_move(
    records: &SurfaceRecords,
    surface: SurfaceId,
    press: PointerPress,
    at: Point,
) -> Option<WindowControlRequest> {
    (records.window_region_at(surface, at)).map(|region| match region {
        WindowRegion::Drag => WindowControlRequest::BeginMove { surface, press },
    })
}
