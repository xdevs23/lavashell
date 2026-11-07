// SPDX-License-Identifier: GPL-3.0-only
use super::control::Control;
use super::owned_wl_surface::OwnedWlSurface;
use super::{Platform, Role, Surface, detach, shape};
use crate::Error;
use lavashell_core::frame::{FrameClock, PresentationTracker};
use lavashell_core::scene::SceneBuilder;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::view::View;
use lavashell_platform::Appearance;
use lavashell_renderer::{Attach, PresentationPolicy, SurfaceAlpha, SurfaceTiming};
use lavashell_runtime::{DragRegion, WindowShape};
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
}

impl Platform {
    /// Opens the record of a surface with its view in the runtime, creates the native surface under the id of that
    /// record, and calls `role` to create the role of its Wayland surface.
    pub(super) fn create_surface(
        &mut self,
        spec: NativeSpec,
        role: impl FnOnce(&mut Self, &wl_surface::WlSurface, SurfaceId) -> Result<Role, Error>,
    ) -> Result<(), Error> {
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
        // The Vulkan target is created before the role. A failure of the target then leaves a Wayland surface without
        // a role, which is safe to destroy.
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
            control: Control::new(id, wl.clone(), wp_viewport, presentation, self.qh.clone()),
        };
        self.renderer.attach(attach).map_err(Error::Render)?;
        let role = role(self, &wl, id).inspect_err(|_| detach(&mut self.renderer, id))?;
        let accessibility =
            crate::accessibility::Bridge::new(id, self.accessibility_events.clone());
        let surface = Surface {
            accessibility: Some(accessibility),
            builder: SceneBuilder::default(),
            role,
            wl,
            _window: window,
            fractional,
            frame_callback_pending: false,
            clock: FrameClock::default(),
            telemetry: PresentationTracker::default(),
            output: None,
            background,
            blur_size: None,
            shape: spec.shape.map(shape::SurfaceShape::new),
        };
        let _replaced_surface = self.surfaces.insert(id, surface);
        Ok(())
    }
}
