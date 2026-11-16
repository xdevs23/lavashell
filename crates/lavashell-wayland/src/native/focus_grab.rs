// SPDX-License-Identifier: GPL-3.0-only
use crate::bind::bind_global;
use crate::focus_grab::protocol::{
    hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1 as Manager,
    hyprland_focus_grab_v1::HyprlandFocusGrabV1 as Grab,
};
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Proxy, QueueHandle};
use std::collections::BTreeMap;

mod error;

pub use error::FocusGrabError;

pub(crate) struct Grabs {
    manager: Manager,
    proxies: BTreeMap<SurfaceId, Grab>,
}

impl Grabs {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Option<Self> {
        bind_global(globals, qh).map(Self::new)
    }

    pub fn new(manager: Manager) -> Self {
        Self {
            manager,
            proxies: BTreeMap::new(),
        }
    }

    pub fn release_and_report_cleared(&mut self, surface: SurfaceId) -> FocusGrabEvent {
        self.release(surface);
        FocusGrabEvent::Cleared(surface)
    }

    pub fn release_if_cleared_by(
        &mut self,
        surface: SurfaceId,
        grab: &Grab,
    ) -> Option<FocusGrabEvent> {
        self.holds(surface, grab)
            .then(|| self.release_and_report_cleared(surface))
    }

    fn holds(&self, surface: SurfaceId, grab: &Grab) -> bool {
        self.proxies.get(&surface) == Some(grab)
    }

    pub fn execute<'a>(
        &mut self,
        request: FocusGrabRequest,
        qh: &QueueHandle<Translator>,
        native: impl Fn(SurfaceId) -> Option<&'a wl_surface::WlSurface>,
    ) -> Result<(), FocusGrabError> {
        match request {
            FocusGrabRequest::Acquire(surface) => {
                let wl = native(surface).ok_or(FocusGrabError::WithoutSurface(surface))?;
                self.acquire(surface, wl, qh);
            }
            FocusGrabRequest::Release(surface) => self.release(surface),
        }
        Ok(())
    }

    fn acquire(
        &mut self,
        surface: SurfaceId,
        wl: &wl_surface::WlSurface,
        qh: &QueueHandle<Translator>,
    ) {
        let grab = self.manager.create_grab(qh, surface);
        grab.add_surface(wl);
        grab.commit();
        if let Some(replaced) = self.proxies.insert(surface, grab) {
            destroy(&replaced);
        }
    }

    fn release(&mut self, surface: SurfaceId) {
        if let Some(grab) = self.proxies.remove(&surface) {
            destroy(&grab);
        }
    }
}

impl Drop for Grabs {
    fn drop(&mut self) {
        self.proxies.values().for_each(destroy);
    }
}

fn destroy(grab: &Grab) {
    if grab.is_alive() {
        grab.destroy();
    }
}
