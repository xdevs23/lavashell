// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use super::layout::{Layout, LayoutSize};
use crate::input::ShownSurface;
use crate::native_window::NativeWindow;
use crate::streams::AospStreams;
use lavashell_core::geometry::PixelSize;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{Reports, SurfaceEvent};

/// A surface in the runtime, drawn by the renderer into the Surface of the host, and the inputs of its layout.
#[derive(Clone, Copy)]
pub(super) struct Shown {
    surface: SurfaceId,
    /// The size of the Surface in pixels.
    window_size: PixelSize,
    /// The density of the display.
    density: Density,
    layout_size: Option<LayoutSize>,
    /// The layout reported last to the surface, or `None` before the Surface first has pixels.
    layout: Option<Layout>,
}

impl Shown {
    /// Shows `surface` in a Surface of `window_size` pixels at `density`, laid out in `layout_size`, and reports the
    /// density and the layout to it.
    ///
    /// # Errors
    /// Fails when the layout size cannot fill the Surface.
    pub(super) fn show(
        surface: SurfaceId,
        window_size: PixelSize,
        density: Density,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<Self, AospError> {
        let mut shown = Self {
            surface,
            window_size,
            density,
            layout_size,
            layout: None,
        };
        shown.change_density(density, reports)?;
        Ok(shown)
    }

    pub(super) fn id(&self) -> SurfaceId {
        self.surface
    }

    /// The shown surface and its layout, once the Surface has had pixels.
    pub(super) fn surface(&self) -> Option<ShownSurface> {
        self.layout.map(|layout| ShownSurface {
            id: self.surface,
            layout,
        })
    }

    /// Lays the surface out in the Surface resized to `window_size` pixels.
    ///
    /// # Errors
    /// Fails when the layout size cannot fill the Surface.
    pub(super) fn resize(
        &mut self,
        window_size: PixelSize,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.window_size = window_size;
        self.lay_out(reports)
    }

    /// Reports `density`, the new density of the display, to the surface, and lays the surface out at `density`.
    ///
    /// # Errors
    /// Fails when the layout size cannot fill the Surface at `density`.
    pub(super) fn change_density(
        &mut self,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.density = density;
        reports.report((self.surface, SurfaceEvent::DensityChanged(density)));
        self.lay_out(reports)
    }

    /// Lays the surface out in `layout_size`.
    ///
    /// # Errors
    /// Fails when `layout_size` cannot fill the Surface.
    pub(super) fn change_layout_size(
        &mut self,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.layout_size = layout_size;
        self.lay_out(reports)
    }

    /// Reports the scale and then the viewport of the layout to the surface. While the Surface has no pixels, `layout`
    /// is unchanged and nothing is reported.
    ///
    /// # Errors
    /// Fails when the layout size cannot fill the Surface.
    fn lay_out(&mut self, reports: &Reports<NativeWindow, AospStreams>) -> Result<(), AospError> {
        let fitted = Layout::fitting(self.window_size, self.density, self.layout_size);
        if let Some(layout) = fitted.map_err(AospError::FitLayout)? {
            let size = layout.viewport.size;
            let configured = SurfaceEvent::Configured {
                width: Some(size.width),
                height: Some(size.height),
                token: None,
            };
            reports.report((self.surface, SurfaceEvent::ScaleChanged(layout.scale)));
            reports.report((self.surface, configured));
            self.layout = Some(layout);
        }
        Ok(())
    }
}
