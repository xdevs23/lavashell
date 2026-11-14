// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use super::layout::LayoutSize;
use super::shown::Shown;
use crate::input::ShownSurface;
use crate::native_window::NativeWindow;
use crate::output::HostOutput;
use crate::platform::vulkan_window;
use crate::streams::AospStreams;
use lavashell_core::geometry::PixelSize;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_platform::{
    PlatformRequest, Reports, SurfaceAttachment, SurfaceEvent, SurfaceRequest, SurfaceSpec,
    TimingEvent,
};
use std::sync::mpsc;
use std::time::Duration;

/// The surface of the runtime, and how far the platform has shown it in the Surface of the host.
#[derive(Default)]
enum RuntimeSurface {
    /// The runtime has not asked for a surface, or the renderer has retired it.
    #[default]
    Absent,
    /// The runtime requested `surface`, and the host has not created a Surface for it yet.
    Requested {
        surface: SurfaceId,
        spec: Box<SurfaceSpec>,
        gpu_budget: Option<Duration>,
    },
    /// The renderer draws the surface into the Surface of the host.
    Shown(Shown),
    /// The surface closed, and is released once the renderer has retired it.
    Closing(SurfaceId),
}

impl RuntimeSurface {
    fn id(&self) -> Option<SurfaceId> {
        match *self {
            Self::Absent => None,
            Self::Requested { surface, .. } | Self::Closing(surface) => Some(surface),
            Self::Shown(shown) => Some(shown.id()),
        }
    }
}

/// The one Surface of an AOSP host and the surface of the runtime shown in it. Each method reports through `reports`.
#[derive(Default)]
pub(crate) struct HostSurface {
    surface: RuntimeSurface,
    /// The native window of the Surface of the host, while the Surface exists.
    window: Option<NativeWindow>,
    /// The size set by the host for the layout of every shown surface, or `None` for the whole Surface at the
    /// density of the display.
    layout_size: Option<LayoutSize>,
    /// The callbacks waiting for the renderer to retire the surface.
    destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostSurface {
    /// A surface requested by `request` is shown at `density`.
    ///
    /// # Errors
    /// Fails for a second surface, for a request that an AOSP host does not support, when the size of the native
    /// window cannot be read, and when the layout size cannot fill the Surface.
    pub(crate) fn execute(
        &mut self,
        request: PlatformRequest,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let request = match request {
            PlatformRequest::Surface(request) => request,
            // The system draws the pointer icon on AOSP, so the host never shows a cursor.
            PlatformRequest::Input(_) => return Ok(()),
            unsupported => return Err(AospError::Unsupported(Box::new(unsupported))),
        };
        match request {
            SurfaceRequest::Create {
                surface,
                spec,
                gpu_budget,
            } => {
                if let Some(open) = self.surface.id() {
                    return Err(AospError::SecondSurface {
                        open,
                        requested: surface,
                    });
                }
                self.surface = RuntimeSurface::Requested {
                    surface,
                    spec,
                    gpu_budget,
                };
                self.show(density, reports)
            }
            SurfaceRequest::Destroy(surface) => {
                self.surface = RuntimeSurface::Closing(surface);
                reports.attach(SurfaceAttachment::Detach(surface));
                Ok(())
            }
            SurfaceRequest::Release(_) => {
                self.surface = RuntimeSurface::Absent;
                for retired in self.destroying.drain(..) {
                    let _returned_callback = retired.send(());
                }
                Ok(())
            }
        }
    }

    /// A requested surface is shown in `window` at `density`.
    ///
    /// # Errors
    /// Fails when the host reports a Surface while another one exists, when the size of `window` cannot be read, and
    /// when the layout size cannot fill the Surface.
    pub(crate) fn created(
        &mut self,
        window: NativeWindow,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if self.window.is_some() {
            return Err(AospError::SecondWindow);
        }
        self.window = Some(window);
        self.show(density, reports)
    }

    /// Lays the shown surface out again in the new size of the Surface of the host.
    ///
    /// # Errors
    /// Fails when the size of the native window cannot be read, and when the layout size cannot fill the Surface.
    pub(crate) fn changed(
        &mut self,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if let (RuntimeSurface::Shown(shown), Some(window)) = (&mut self.surface, &self.window) {
            shown.resize(window_size(window)?, reports)?;
        }
        Ok(())
    }

    /// Reports `density`, the new density of the display, to the shown surface, and lays it out again.
    ///
    /// # Errors
    /// Fails when the layout size cannot fill the Surface at `density`.
    pub(crate) fn density_changed(
        &mut self,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if let RuntimeSurface::Shown(shown) = &mut self.surface {
            shown.change_density(density, reports)?;
        }
        Ok(())
    }

    /// Lays the shown surface and every later one out in `layout_size`.
    ///
    /// # Errors
    /// Fails when `layout_size` cannot fill the Surface.
    pub(crate) fn layout_size_changed(
        &mut self,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.layout_size = layout_size;
        if let RuntimeSurface::Shown(shown) = &mut self.surface {
            shown.change_layout_size(layout_size, reports)?;
        }
        Ok(())
    }

    /// Closes the surface shown in the destroyed Surface of the host. Sends on `retired` after the renderer has
    /// retired the surface. Sends at once if the renderer never drew into the Surface.
    pub(crate) fn destroyed(
        &mut self,
        retired: mpsc::SyncSender<()>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) {
        drop(self.window.take());
        match self.surface {
            RuntimeSurface::Shown(shown) => {
                let surface = shown.id();
                reports.report((surface, SurfaceEvent::Closed));
                self.surface = RuntimeSurface::Closing(surface);
                self.destroying.push(retired);
            }
            RuntimeSurface::Closing(_) => self.destroying.push(retired),
            RuntimeSurface::Absent | RuntimeSurface::Requested { .. } => {
                let _returned_callback = retired.send(());
            }
        }
    }

    /// Reports that the display is ready for the next frame of the shown surface.
    pub(crate) fn frame(&self, reports: &Reports<NativeWindow, AospStreams>) {
        if let RuntimeSurface::Shown(shown) = &self.surface {
            reports.time(TimingEvent::DisplayReady(shown.id()));
        }
    }

    /// The shown surface and its layout, once the Surface has had pixels.
    pub(crate) fn shown(&self) -> Option<ShownSurface> {
        let RuntimeSurface::Shown(shown) = &self.surface else {
            return None;
        };
        shown.surface()
    }

    /// The callbacks waiting for the renderer to retire the surface.
    pub(crate) fn into_destroying(self) -> Vec<mpsc::SyncSender<()>> {
        self.destroying
    }

    /// Attaches the requested surface to the native window once both exist. Shows it at `density` and reports it as
    /// shown on the display.
    fn show(
        &mut self,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let (
            RuntimeSurface::Requested {
                surface,
                spec,
                gpu_budget,
            },
            Some(window),
        ) = (&self.surface, &self.window)
        else {
            return Ok(());
        };
        let size = window_size(window)?;
        let timing = window.timing.clone();
        let attach = spec.attach(
            *surface,
            vulkan_window(window),
            timing,
            *gpu_budget,
            window.clone(),
        );
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        let shown = Shown::show(*surface, size, density, self.layout_size, reports)?;
        HostOutput::entered(shown.id(), reports);
        self.surface = RuntimeSurface::Shown(shown);
        Ok(())
    }
}

fn window_size(window: &NativeWindow) -> Result<PixelSize, AospError> {
    let (width, height) = window.size().map_err(AospError::WindowSize)?;
    Ok(PixelSize::new(width, height))
}
