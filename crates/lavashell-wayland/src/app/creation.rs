// SPDX-License-Identifier: GPL-3.0-only
use super::owned_wl_surface::OwnedWlSurface;
use super::{
    DamageTracker, Density, FrameClock, HdrMetadata, OutputRequest, Platform, PresentationTracker,
    Role, SceneTree, Surface, View, shape, wl_surface,
};
use crate::Error;
use lavashell_core::surface::SurfaceId;
use lavashell_renderer::SurfaceAlpha;
use std::sync::Arc;

/// The view and the settings of a new native surface, apart from its role.
pub(super) struct SurfaceSpec {
    pub view: View,
    pub size: (u32, u32),
    pub transparent: bool,
    pub output: OutputRequest,
    pub hdr_metadata: Option<HdrMetadata>,
    pub background_blur: bool,
    pub shape: Option<crate::WindowShape>,
    pub drag_region: Option<crate::surface::DragRegion>,
    pub accessible_name: String,
}

impl Platform {
    /// Opens the record of a surface with its view in the runtime, creates the native surface under the id of that
    /// record, and calls `role` to create the role of its Wayland surface.
    pub(super) fn create_surface(
        &mut self,
        spec: SurfaceSpec,
        role: impl FnOnce(&mut Self, &wl_surface::WlSurface, SurfaceId) -> Result<Role, Error>,
    ) -> Result<(), Error> {
        let supplied_facts = self.view_configuration.apply(&spec.view, self.start);
        let size = crate::logical::size(spec.size);
        let id = self.surface_records.open(spec.view, size, Density::ONE)?;
        let wl = self.compositor.create_surface(&self.qh);
        let window = Arc::new(OwnedWlSurface::new(wl.clone(), self.connection.backend()));
        let protocols = &self.protocols;
        let wp_viewport = (protocols.viewporter.as_ref())
            .map(|viewporter| viewporter.get_viewport(&wl, &self.qh, ()));
        let fractional = (wp_viewport.as_ref().and(protocols.fractional.as_ref()))
            .map(|manager| manager.get_fractional_scale(&wl, &self.qh, id));
        let background = (protocols.background.as_ref())
            .filter(|_| spec.background_blur)
            .map(|manager| manager.get_background_effect(&wl, &self.qh, ()));
        if spec.background_blur && background.is_none() {
            log::warn!(
                "the compositor offers no background blur for surface {}",
                id.0
            );
        }
        // The Vulkan target is created before the role. A failure of the target then leaves a Wayland surface without
        // a role, which is safe to destroy.
        let alpha = if spec.transparent {
            SurfaceAlpha::Transparent
        } else {
            SurfaceAlpha::Opaque
        };
        let mut target =
            (self.gpu.wayland_target(window.clone(), alpha)).map_err(Error::CreateTarget)?;
        target.request_output(spec.output);
        if let Some(metadata) = spec.hdr_metadata {
            target
                .hdr_metadata(metadata)
                .map_err(Error::SetHdrMetadata)?;
        }
        let role = role(self, &wl, id)?;
        let accessibility = crate::accessibility::Bridge::new(
            id,
            spec.accessible_name,
            self.accessibility_events.clone(),
        );
        let surface = Surface {
            accessibility: Some(accessibility),
            scene_generation: 0,
            target: Some(target),
            tree: SceneTree::default(),
            damage: DamageTracker::default(),
            role: Some(role),
            wl,
            window: Some(window),
            wp_viewport,
            fractional,
            supplied_facts,
            pointer_owner: None,
            frame_request: None,
            frame_callback: None,
            feedback: Vec::new(),
            clock: FrameClock::default(),
            telemetry: PresentationTracker::default(),
            output: None,
            background,
            blur_size: None,
            shape: spec.shape.map(shape::SurfaceShape::new),
            drag_region: spec.drag_region,
        };
        let _replaced_surface = self.surfaces.insert(id, surface);
        Ok(())
    }
}
