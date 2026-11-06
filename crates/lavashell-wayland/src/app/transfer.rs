// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{DragSourceEvent, DragTargetEvent};
use sctk::reexports::client::protocol::wl_surface;
use smithay_client_toolkit as sctk;

impl Platform {
    /// Hands `event` about a drag offered to the app to the runtime.
    pub(crate) fn drag_target_event(&mut self, event: DragTargetEvent) {
        self.config
            .drags
            .handle_target(event, &mut self.surface_records);
    }

    /// Hands `event` about a drag started by the app to the runtime.
    pub(crate) fn drag_source_event(&mut self, event: DragSourceEvent) {
        self.config
            .drags
            .handle_source(event, &mut self.surface_records);
    }

    /// Ends every drag and every drag transfer of the runtime, because the app closes.
    pub(crate) fn close_drags(&mut self) {
        self.config.drags.close(&mut self.surface_records);
    }

    /// The id of the open surface `surface`.
    pub(crate) fn transfer_surface(&self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        (self.find_surface(surface)).filter(|&id| self.surface_records.is_open(id))
    }

    /// The native surface of the open surface `surface`.
    pub(crate) fn open_wl_surface(&self, surface: SurfaceId) -> Option<wl_surface::WlSurface> {
        (self.surfaces.get(&surface))
            .filter(|_| self.surface_records.is_open(surface))
            .map(|known| known.wl.clone())
    }
}
