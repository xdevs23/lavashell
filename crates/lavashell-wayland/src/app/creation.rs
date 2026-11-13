// SPDX-License-Identifier: GPL-3.0-only
use super::control::{Control, RoleControl, SurfaceObjects};
use super::layer::{self, ProtocolLayout};
use super::owned_wl_surface::OwnedWlSurface;
use super::{NativeRole, Surface, destroy_objects};
use crate::Error;
use crate::LayerError;
use crate::translator::{Translator, ViewLink};
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_platform::{Role, SurfaceAttachment, SurfaceEvent, SurfaceRequest, SurfaceSpec};
use lavashell_renderer::{PresentationPolicy, SurfaceTiming};
use sctk::reexports::client::protocol::wl_surface;
use smithay_client_toolkit as sctk;
use std::sync::Arc;
use std::time::Duration;

impl Translator {
    /// Executes a request of the runtime for a native surface. A destroyed surface reports no more events and
    /// detaches from the renderer. The native surface remains until `Release` arrives, after nothing presents to it
    /// anymore.
    pub(crate) fn execute_surface(&mut self, request: SurfaceRequest) -> Result<(), Error> {
        match request {
            SurfaceRequest::Create {
                surface,
                spec,
                gpu_budget,
            } => self.create_surface(surface, &spec, gpu_budget),
            SurfaceRequest::Destroy(surface) => {
                if let Some(retired) = self.surfaces.retire(surface) {
                    retired.accessibility = None;
                }
                self.links.attach(SurfaceAttachment::Detach(surface));
                Ok(())
            }
            SurfaceRequest::Release(surface) => {
                let _released = self.surfaces.release(surface);
                Ok(())
            }
        }
    }

    /// Creates the native surface of `surface` as `spec` describes, and attaches it to the renderer with
    /// `gpu_budget`. A surface whose output is gone closes at once.
    fn create_surface(
        &mut self,
        surface: SurfaceId,
        spec: &SurfaceSpec,
        gpu_budget: Option<Duration>,
    ) -> Result<(), Error> {
        let wl = self.compositor.create_surface(&self.qh);
        let window = Arc::new(OwnedWlSurface::new(wl.clone(), self.connection.backend()));
        let protocols = &self.protocols;
        let wp_viewport = (protocols.viewporter.as_ref())
            .map(|viewporter| viewporter.get_viewport(&wl, &self.qh, ()));
        let fractional = (wp_viewport.as_ref().and(protocols.fractional.as_ref()))
            .map(|manager| manager.get_fractional_scale(&wl, &self.qh, surface));
        let background = (protocols.background.as_ref())
            .filter(|_| spec.appearance.background_blur)
            .map(|manager| manager.get_background_effect(&wl, &self.qh, ()));
        if spec.appearance.background_blur && background.is_none() {
            log::warn!(
                "the compositor offers no background blur for surface {}",
                surface.0
            );
        }
        // The control of the Vulkan target needs the layer object of a layer role, so the role is created first.
        let (role, role_control) = match self.create_role(&wl, surface, &spec.role) {
            Ok(Some(created)) => created,
            Ok(None) => {
                destroy_objects(wp_viewport, fractional, background, None);
                self.links.report((surface, SurfaceEvent::Closed));
                return Ok(());
            }
            Err(error) => {
                destroy_objects(wp_viewport, fractional, background, None);
                return Err(error);
            }
        };
        let presentation =
            (self.presentation.as_ref()).map(|presentation| presentation.proxy.clone());
        let control = Control::new(
            surface,
            SurfaceObjects {
                connection: self.connection.clone(),
                wl: wl.clone(),
                viewport: wp_viewport.clone(),
                presentation,
                compositor: self.compositor.wl_compositor().clone(),
                background: background.clone(),
                role_control,
            },
            role_state(&spec.role),
            self.qh.clone(),
        );
        let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
        let attach = spec.attach(surface, window.clone(), timing, gpu_budget, control);
        let accessibility =
            crate::accessibility::Bridge::new(surface, ViewLink::new(self.links.view.clone()));
        let native = Surface {
            accessibility: Some(accessibility),
            role,
            wl,
            _window: window,
            viewport: wp_viewport,
            fractional,
            background,
        };
        self.surfaces.insert(surface, native);
        self.links
            .attach(SurfaceAttachment::Attach(Box::new(attach)));
        Ok(())
    }

    /// Gives `wl` the role that `role` describes, with the control of its state. Returns `None` when the output of
    /// the role is gone.
    fn create_role(
        &self,
        wl: &wl_surface::WlSurface,
        surface: SurfaceId,
        role: &Role,
    ) -> Result<Option<(NativeRole, RoleControl)>, Error> {
        Ok(Some(match role {
            Role::Window(window) => self.window_role(wl, window)?,
            Role::Panel(panel) => {
                let output = match panel.output.map(|output| self.find_output(output)) {
                    Some(None) => return Ok(None),
                    chosen => chosen.flatten(),
                };
                let layout = ProtocolLayout::new(panel.geometry).map_err(LayerError::Layout)?;
                let proxy = self.layer_surface(wl, surface, output.as_ref(), panel)?;
                layer::panel_role(wl, proxy, layout, panel.keyboard)
            }
            Role::Lock { output } => {
                let Some(output) = self.find_output(*output) else {
                    return Ok(None);
                };
                self.lock_role(wl, &output, surface)?
            }
        }))
    }
}

/// The state that the role of a native surface sends to the host when it is created.
fn role_state(role: &Role) -> SurfaceState {
    match role {
        Role::Panel(panel) => layer::role_state(panel.geometry),
        Role::Window(_) | Role::Lock { .. } => SurfaceState::default(),
    }
}
