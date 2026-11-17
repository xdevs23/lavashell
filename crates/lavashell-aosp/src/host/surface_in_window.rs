// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use crate::input::ShownSurface;
use crate::layout_size::{LayoutSize, fit_viewport_into_window};
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_core::geometry::{PixelSize, ScaledViewport};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::window::WindowState;
use lavashell_platform::{Reports, SurfaceEvent};

pub(super) struct SurfaceInWindow {
    id: SurfaceId,
    window_size: PixelSize,
    window: Option<WindowState>,
    scaled_viewport: Option<ScaledViewport>,
}

impl SurfaceInWindow {
    pub(super) fn show(
        id: SurfaceId,
        window_size: PixelSize,
        window: Option<WindowState>,
        density: Density,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<Self, AospError> {
        let mut shown = Self {
            id,
            window_size,
            window,
            scaled_viewport: None,
        };
        shown.change_density(density, layout_size, reports)?;
        Ok(shown)
    }

    pub(super) fn id(&self) -> SurfaceId {
        self.id
    }

    pub(super) fn input_target(&self) -> Option<ShownSurface> {
        self.scaled_viewport.map(|scaled_viewport| ShownSurface {
            id: self.id,
            scaled_viewport,
        })
    }

    pub(super) fn resize(
        &mut self,
        window_size: PixelSize,
        density: Density,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.window_size = window_size;
        self.lay_out(density, layout_size, reports)
    }

    pub(super) fn change_density(
        &mut self,
        density: Density,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        reports.report((self.id, SurfaceEvent::DensityChanged(density)));
        self.lay_out(density, layout_size, reports)
    }

    pub(super) fn lay_out(
        &mut self,
        density: Density,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let fitted = fit_viewport_into_window(self.window_size, density, layout_size);
        if let Some(scaled_viewport) = fitted.map_err(AospError::FitLayout)? {
            let size = scaled_viewport.viewport.size;
            let configured = SurfaceEvent::Configured {
                width: Some(size.width),
                height: Some(size.height),
                token: None,
                window: self.window,
            };
            reports.report((self.id, SurfaceEvent::ScaleChanged(scaled_viewport.scale)));
            reports.report((self.id, configured));
            self.scaled_viewport = Some(scaled_viewport);
        }
        Ok(())
    }
}
