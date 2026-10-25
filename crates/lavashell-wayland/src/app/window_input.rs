// SPDX-License-Identifier: GPL-3.0-only
use super::{InputEvent, Platform, Point, Role, wl_pointer};

impl Platform {
    pub(super) fn move_window(
        &mut self,
        index: usize,
        pointer: &wl_pointer::WlPointer,
        at: Point,
        serial: u32,
    ) -> bool {
        let Some(seat) = self.transfer_pointer_device(pointer) else {
            return false;
        };
        let surface = &mut self.surfaces[index];
        let size = surface.logical_size();
        if surface
            .drag_region
            .as_ref()
            .is_none_or(|region| !region(at, size))
        {
            return false;
        }
        let Some(Role::Window { window }) = surface.lifecycle.role() else {
            return false;
        };
        window.move_(&seat, serial);
        surface.view.input(InputEvent::CancelPointer);
        true
    }
}
