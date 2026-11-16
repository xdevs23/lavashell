// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::FocusGrabError;
use crate::app::Surface;
use crate::focus_grab::protocol::{
    hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1 as Manager,
    hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1 as Grab},
};
use crate::sctk;
use lavashell_core::surface::SurfaceId;
use lavashell_platform::FocusGrabRequest;
use sctk::reexports::client::{Connection, Dispatch, QueueHandle, delegate_noop};

impl Translator {
    pub(super) fn execute_focus_grab(
        &mut self,
        request: FocusGrabRequest,
    ) -> Result<(), FocusGrabError> {
        let grabs = (self.focus_grabs.as_mut()).ok_or(FocusGrabError::NoFocusGrab)?;
        grabs.execute(request, &self.qh, |surface| {
            self.surfaces.live(surface).map(Surface::wl)
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
            && let Some(cleared) = grabs.release_if_cleared_by(*surface, proxy)
        {
            state.links.report(cleared);
        }
    }
}

delegate_noop!(Translator: ignore Manager);
