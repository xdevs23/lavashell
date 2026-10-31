// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, Platform, Point, Role, SurfaceId, open_surface, wl_pointer};

impl Platform {
    pub(super) fn move_window(
        &mut self,
        surface: SurfaceId,
        pointer: &wl_pointer::WlPointer,
        at: Point,
        serial: u32,
    ) -> bool {
        let Some(seat) = self.transfer_pointer_device(pointer) else {
            return false;
        };
        let Ok((native, view, viewport)) =
            open_surface(&mut self.surfaces, &mut self.surface_records, surface)
        else {
            return false;
        };
        if native
            .drag_region
            .as_ref()
            .is_none_or(|region| !region(at, viewport.size))
        {
            return false;
        }
        let Some(Role::Window { window }) = &native.role else {
            return false;
        };
        window.move_(&seat, serial);
        view.input(InputEvent::CancelPointer);
        true
    }
}
