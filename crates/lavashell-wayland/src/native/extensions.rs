// SPDX-License-Identifier: GPL-3.0-only
use super::SurfaceProtocols;
use crate::sctk;
use crate::translator::Translator;
use lavashell_core::surface::SurfaceId;
use sctk::reexports::client::QueueHandle;
use sctk::reexports::client::protocol::wl_surface;
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1;
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use sctk::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;

pub(crate) struct SurfaceExtensions {
    pub(super) viewport: Option<WpViewport>,
    pub(super) fractional: Option<WpFractionalScaleV1>,
    pub(super) background: Option<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
}

impl SurfaceExtensions {
    pub(crate) fn create(
        protocols: &SurfaceProtocols,
        wl: &wl_surface::WlSurface,
        qh: &QueueHandle<Translator>,
        surface: SurfaceId,
        background_blur: bool,
    ) -> Self {
        let viewport =
            (protocols.viewporter.as_ref()).map(|viewporter| viewporter.get_viewport(wl, qh, ()));
        let fractional = (viewport.as_ref().and(protocols.fractional.as_ref()))
            .map(|manager| manager.get_fractional_scale(wl, qh, surface));
        let background = (protocols.background.as_ref())
            .filter(|_| background_blur)
            .map(|manager| manager.get_background_effect(wl, qh, ()));
        if background_blur && background.is_none() {
            log::warn!(
                "the compositor offers no background blur for surface {}",
                surface.get()
            );
        }
        Self {
            viewport,
            fractional,
            background,
        }
    }
}

impl Drop for SurfaceExtensions {
    fn drop(&mut self) {
        if let Some(viewport) = &self.viewport {
            viewport.destroy();
        }
        if let Some(background) = &self.background {
            background.destroy();
        }
        if let Some(fractional) = &self.fractional {
            fractional.destroy();
        }
    }
}
