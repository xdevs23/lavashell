// SPDX-License-Identifier: GPL-3.0-only
use super::control::{Control, RoleControl, SurfaceObjects};
use super::owned_wl_surface::OwnedWlSurface;
use super::render::FrameProgress;
use super::{Platform, Role, Surface, destroy_objects};
use crate::Error;
use lavashell_core::surface::{SurfaceId, SurfaceState};
use lavashell_core::units::Density;
use lavashell_core::view::View;
use lavashell_platform::Appearance;
use lavashell_renderer::{Attach, PresentationPolicy, SurfaceAlpha, SurfaceTiming};
use lavashell_runtime::{DragRegion, RendererInput, SchedulerInput, SurfaceShape, WindowShape};
use sctk::reexports::client::protocol::wl_surface;
use smithay_client_toolkit as sctk;
use std::sync::Arc;

/// The view and the settings of a new native surface, apart from its role.
pub(super) struct NativeSpec {
    pub view: View,
    pub size: (u32, u32),
    pub appearance: Appearance,
    pub shape: Option<WindowShape>,
    pub drag_region: Option<DragRegion>,
    pub accessible_name: String,
    /// The state of the surface sent to the host with its role.
    pub role_state: SurfaceState,
}

impl Platform {
    /// Opens the record of a surface with its view in the runtime, creates the native surface under the id of that
    /// record, calls `create_role` to create the role of its Wayland surface with the control of the role, and attaches
    /// the surface to the renderer. Returns the id of the new surface.
    pub(super) fn create_surface(
        &mut self,
        spec: NativeSpec,
        create_role: impl FnOnce(
            &mut Self,
            &wl_surface::WlSurface,
            SurfaceId,
        ) -> Result<(Role, RoleControl), Error>,
    ) -> Result<SurfaceId, Error> {
        let start = self.start;
        spec.view.motion().time_source(move || start.elapsed());
        let size = crate::logical::size(spec.size);
        let id = (self.surface_records).open(
            spec.view,
            size,
            Density::ONE,
            spec.drag_region,
            spec.accessible_name,
        )?;
        let wl = self.compositor.create_surface(&self.qh);
        let window = Arc::new(OwnedWlSurface::new(wl.clone(), self.connection.backend()));
        let protocols = &self.protocols;
        let wp_viewport = (protocols.viewporter.as_ref())
            .map(|viewporter| viewporter.get_viewport(&wl, &self.qh, ()));
        let fractional = (wp_viewport.as_ref().and(protocols.fractional.as_ref()))
            .map(|manager| manager.get_fractional_scale(&wl, &self.qh, id));
        let background = (protocols.background.as_ref())
            .filter(|_| spec.appearance.background_blur)
            .map(|manager| manager.get_background_effect(&wl, &self.qh, ()));
        if spec.appearance.background_blur && background.is_none() {
            log::warn!(
                "the compositor offers no background blur for surface {}",
                id.0
            );
        }
        // The control of the Vulkan target needs the layer object of a layer role, so the role is created first.
        let (role, role_control) = match create_role(self, &wl, id) {
            Ok(created) => created,
            Err(error) => {
                destroy_objects(wp_viewport, fractional, background, None);
                return Err(error);
            }
        };
        let alpha = if spec.appearance.transparent {
            SurfaceAlpha::Transparent
        } else {
            SurfaceAlpha::Opaque
        };
        let presentation =
            (self.presentation.as_ref()).map(|presentation| presentation.proxy.clone());
        let attach = Attach {
            surface: id,
            window: window.clone(),
            alpha,
            timing: SurfaceTiming::new(PresentationPolicy::Automatic),
            output: spec.appearance.output,
            hdr_metadata: spec.appearance.hdr_metadata,
            gpu_budget: self.config.ui_budget,
            control: Control::new(
                id,
                SurfaceObjects {
                    connection: self.connection.clone(),
                    wl: wl.clone(),
                    viewport: wp_viewport.clone(),
                    presentation,
                    compositor: self.compositor.wl_compositor().clone(),
                    background: background.clone(),
                    role_control,
                },
                spec.role_state,
                self.qh.clone(),
            ),
        };
        let accessibility =
            crate::accessibility::Bridge::new(id, self.accessibility_events.clone());
        let surface = Surface {
            accessibility: Some(accessibility),
            frame: FrameProgress::Idle,
            last_frame: None,
            committing: None,
            role,
            wl,
            _window: window,
            viewport: wp_viewport,
            fractional,
            output: None,
            background,
            shape: SurfaceShape::new(spec.shape),
        };
        let _replaced_surface = self.surfaces.insert(id, surface);
        self.schedule(SchedulerInput::Opened(id));
        self.renderer
            .send(RendererInput::Attach(Box::new(attach)))?;
        Ok(id)
    }
}
