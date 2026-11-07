// SPDX-License-Identifier: GPL-3.0-only
use super::Platform;
use crate::Error;
use crate::focus_grab::protocol::{
    hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1 as Manager,
    hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1 as Grab},
};
use lavashell_core::focus_grab::FocusGrab;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use lavashell_runtime::{FocusGrabs, PanelRecords, SurfaceRecords};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;

/// The focus grabs of the layers: the rules from the runtime, and the grabs made by the compositor for them.
pub(super) struct Grabs {
    rules: FocusGrabs,
    manager: Manager,
    proxies: BTreeMap<SurfaceId, Grab>,
}

impl Grabs {
    /// The focus grabs through the manager of the compositor, or `None` when the compositor offers no focus grab.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Platform>) -> Option<Self> {
        globals.bind(qh, 1..=1, ()).ok().map(Self::new)
    }

    pub fn new(manager: Manager) -> Self {
        Self {
            rules: FocusGrabs::default(),
            manager,
            proxies: BTreeMap::new(),
        }
    }

    pub fn open(&mut self, surface: SurfaceId, grab: FocusGrab) {
        self.rules.open(surface, grab);
    }

    /// Removes the focus grab of `surface` and ends the grab of the compositor, if the surface held one. `native`
    /// returns the native surface of a layer.
    pub fn forget<'a>(
        &mut self,
        surface: SurfaceId,
        qh: &QueueHandle<Platform>,
        native: impl Fn(SurfaceId) -> Option<&'a wl_surface::WlSurface>,
    ) -> Result<(), Error> {
        if let Some(request) = self.rules.forget(surface) {
            self.execute(request, qh, &native)?;
        }
        Ok(())
    }

    /// Updates the focus grabs of the layers and executes the requests of the runtime. `native` returns the native
    /// surface of a layer.
    ///
    /// # Errors
    ///
    /// Fails when the runtime asks for a grab on a surface without a native surface.
    pub fn update<'a>(
        &mut self,
        panels: &PanelRecords,
        surfaces: &SurfaceRecords,
        qh: &QueueHandle<Platform>,
        native: impl Fn(SurfaceId) -> Option<&'a wl_surface::WlSurface>,
    ) -> Result<(), Error> {
        for request in self.rules.update(panels, surfaces) {
            self.execute(request, qh, &native)?;
        }
        Ok(())
    }

    pub fn cleared(&mut self, surface: SurfaceId) {
        self.release(surface);
        self.rules.handle(FocusGrabEvent::Cleared(surface));
    }

    pub fn cleared_by(&mut self, surface: SurfaceId, grab: &Grab) {
        if self.holds(surface, grab) {
            self.cleared(surface);
        }
    }

    /// Whether `grab` is the grab of the compositor for `surface`. A grab that was released before the compositor
    /// cleared it is not.
    fn holds(&self, surface: SurfaceId, grab: &Grab) -> bool {
        self.proxies.get(&surface) == Some(grab)
    }

    fn execute<'a>(
        &mut self,
        request: FocusGrabRequest,
        qh: &QueueHandle<Platform>,
        native: &impl Fn(SurfaceId) -> Option<&'a wl_surface::WlSurface>,
    ) -> Result<(), Error> {
        match request {
            FocusGrabRequest::Acquire(surface) => {
                let wl = native(surface).ok_or(Error::FocusGrabWithoutSurface(surface))?;
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
        qh: &QueueHandle<Platform>,
    ) {
        let grab = self.manager.create_grab(qh, surface);
        grab.add_surface(wl);
        grab.commit();
        let _replaced_grab = self.proxies.insert(surface, grab);
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

impl Platform {
    pub(super) fn open_focus_grab(&mut self, surface: SurfaceId, grab: FocusGrab) {
        if let Some(grabs) = &mut self.focus_grabs {
            grabs.open(surface, grab);
        }
    }

    pub(super) fn forget_focus_grab(&mut self, surface: SurfaceId) -> Result<(), Error> {
        let Some(grabs) = &mut self.focus_grabs else {
            return Ok(());
        };
        grabs.forget(surface, &self.qh, |surface| {
            self.surfaces.get(&surface).map(|native| &native.wl)
        })
    }

    pub(super) fn update_focus_grabs(&mut self) -> Result<(), Error> {
        let Some(grabs) = &mut self.focus_grabs else {
            return Ok(());
        };
        grabs.update(&self.panels, &self.surface_records, &self.qh, |surface| {
            self.surfaces.get(&surface).map(|native| &native.wl)
        })
    }
}

impl Dispatch<Grab, SurfaceId> for Platform {
    fn event(
        state: &mut Self,
        proxy: &Grab,
        event: hyprland_focus_grab_v1::Event,
        surface: &SurfaceId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let hyprland_focus_grab_v1::Event::Cleared = event
            && let Some(grabs) = &mut state.focus_grabs
        {
            grabs.cleared_by(*surface, proxy);
        }
    }
}

delegate_noop!(Platform: ignore Manager);
