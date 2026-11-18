// SPDX-License-Identifier: GPL-3.0-only
use crate::registry::Subsystem;
use crate::run::RuntimeSetup;
use crate::surface_records::SurfaceRecords;
use lavashell_core::geometry::Point;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{Capability, PointerPress, WindowControlRequest};

inventory::submit! { Subsystem { capability: Capability::WindowMove, used: has_drag_region } }

fn has_drag_region(setup: &RuntimeSetup) -> bool {
    setup.windows().any(|window| window.drag_region.is_some())
}

pub(crate) fn begin_move(
    records: &SurfaceRecords,
    surface: SurfaceId,
    press: PointerPress,
    at: Point,
) -> Option<WindowControlRequest> {
    (records.in_drag_region(surface, at))
        .then_some(WindowControlRequest::BeginMove { surface, press })
}
