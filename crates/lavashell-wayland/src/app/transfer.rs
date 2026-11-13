// SPDX-License-Identifier: GPL-3.0-only
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use sctk::reexports::client::protocol::wl_surface;
use smithay_client_toolkit as sctk;

impl Translator {
    /// The native surface of the surface `surface`, until the runtime destroys it.
    pub(crate) fn open_wl_surface(&self, surface: SurfaceId) -> Option<wl_surface::WlSurface> {
        self.surfaces.live(surface).map(|known| known.wl.clone())
    }
}
