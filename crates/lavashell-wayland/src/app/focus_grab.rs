// SPDX-License-Identifier: GPL-3.0-only
use crate::focus_grab::protocol::{
    hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1 as Manager,
    hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1 as Grab},
};
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::{FocusGrabEvent, FocusGrabRequest};
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use smithay_client_toolkit as sctk;
use std::collections::BTreeMap;

mod error;

pub use error::FocusGrabError;

/// The focus grabs that the compositor made for the layers.
pub(crate) struct Grabs {
    manager: Manager,
    proxies: BTreeMap<SurfaceId, Grab>,
}

impl Grabs {
    /// The focus grabs through the manager of the compositor, or `None` when the compositor offers no focus grab.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Translator>) -> Option<Self> {
        globals.bind(qh, 1..=1, ()).ok().map(Self::new)
    }

    pub fn new(manager: Manager) -> Self {
        Self {
            manager,
            proxies: BTreeMap::new(),
        }
    }

    /// Ends the grab of `surface`, which the compositor cleared, and returns the event that reports the clear.
    pub fn cleared(&mut self, surface: SurfaceId) -> FocusGrabEvent {
        self.release(surface);
        FocusGrabEvent::Cleared(surface)
    }

    /// Ends the grab of `surface` when the compositor cleared `grab`, the grab used by the surface, and returns the
    /// event that reports the clear.
    pub fn cleared_by(&mut self, surface: SurfaceId, grab: &Grab) -> Option<FocusGrabEvent> {
        self.holds(surface, grab).then(|| self.cleared(surface))
    }

    /// Whether `grab` is the grab of the compositor for `surface`. A grab that was released before the compositor
    /// cleared it is not.
    fn holds(&self, surface: SurfaceId, grab: &Grab) -> bool {
        self.proxies.get(&surface) == Some(grab)
    }

    /// Executes `request` of the runtime. `native` returns the native surface of a layer.
    ///
    /// # Errors
    ///
    /// Fails when the runtime asks for a grab on a surface without a native surface.
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

impl Translator {
    /// Executes `request` of the runtime on the focus grabs. On a compositor without focus grabs, every request is
    /// ignored.
    pub(crate) fn execute_focus_grab(
        &mut self,
        request: FocusGrabRequest,
    ) -> Result<(), FocusGrabError> {
        let Some(grabs) = &mut self.focus_grabs else {
            return Ok(());
        };
        grabs.execute(request, &self.qh, |surface| {
            self.surfaces.live(surface).map(|native| &native.wl)
        })
    }
}

impl Dispatch<Grab, SurfaceId> for Translator {
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
            && let Some(cleared) = grabs.cleared_by(*surface, proxy)
        {
            state.links.report(cleared);
        }
    }
}

delegate_noop!(Translator: ignore Manager);
