// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use crate::input::ShownSurface;
use crate::native_window::NativeWindow;
use crate::output::HostOutput;
use crate::platform::vulkan_window;
use crate::streams::AospStreams;
use lavashell_core::geometry::{PixelScale, Size};
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Px};
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
    /// The renderer draws the surface into the Surface of the host, which has `size` in pixels, at the `density` of
    /// the display.
    Shown {
        surface: SurfaceId,
        size: (u32, u32),
        density: Density,
    },
    /// The surface closed, and is released once the renderer has retired it.
    Closing(SurfaceId),
}

impl RuntimeSurface {
    fn id(&self) -> Option<SurfaceId> {
        match *self {
            Self::Absent => None,
            Self::Requested { surface, .. }
            | Self::Shown { surface, .. }
            | Self::Closing(surface) => Some(surface),
        }
    }
}

/// The one Surface of an AOSP host and the surface of the runtime shown in it. Each method reports through `reports`.
#[derive(Default)]
pub(crate) struct HostSurface {
    surface: RuntimeSurface,
    /// The native window of the Surface of the host, while the Surface exists.
    window: Option<NativeWindow>,
    /// The callbacks waiting for the renderer to retire the surface.
    destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostSurface {
    /// A surface requested by `request` is shown at `density`.
    ///
    /// # Errors
    /// Fails for a second surface, for a request that an AOSP host does not support, and when the size of the
    /// native window cannot be read.
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
    /// Fails when the host reports a Surface while another one exists, and when the size of `window` cannot be read.
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

    /// Reports the new size of the Surface of the host to the shown surface.
    ///
    /// # Errors
    /// Fails when the size of the native window cannot be read.
    pub(crate) fn changed(
        &mut self,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if let (
            RuntimeSurface::Shown {
                surface,
                size,
                density,
            },
            Some(window),
        ) = (&mut self.surface, &self.window)
        {
            *size = window_size(window)?;
            configure(reports, *surface, *size, *density);
        }
        Ok(())
    }

    /// Reports `density`, the new density of the display, to the shown surface.
    pub(crate) fn density_changed(
        &mut self,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) {
        if let RuntimeSurface::Shown { surface, size, .. } = self.surface {
            self.surface = apply_density(reports, surface, size, density);
        }
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
            RuntimeSurface::Shown { surface, .. } => {
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
        if let RuntimeSurface::Shown { surface, .. } = self.surface {
            reports.time(TimingEvent::DisplayReady(surface));
        }
    }

    /// The shown surface, the size of the Surface, and the density of the display.
    pub(crate) fn shown(&self) -> Option<ShownSurface> {
        let RuntimeSurface::Shown {
            surface,
            size,
            density,
        } = self.surface
        else {
            return None;
        };
        Some(ShownSurface {
            id: surface,
            size: surface_size(size, density),
            density,
        })
    }

    /// The callbacks waiting for the renderer to retire the surface.
    pub(crate) fn into_destroying(self) -> Vec<mpsc::SyncSender<()>> {
        self.destroying
    }

    /// Attaches the requested surface to the native window once both exist, shows it at `density` and reports it as
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
        let surface = *surface;
        let size = window_size(window)?;
        let timing = window.timing.clone();
        let attach = spec.attach(
            surface,
            vulkan_window(window),
            timing,
            *gpu_budget,
            window.clone(),
        );
        reports.attach(SurfaceAttachment::Attach {
            attach: Box::new(attach),
            scale: PixelScale::SQUARE,
        });
        self.surface = apply_density(reports, surface, size, density);
        HostOutput::entered(surface, reports);
        Ok(())
    }
}

fn window_size(window: &NativeWindow) -> Result<(u32, u32), AospError> {
    window.size().map_err(AospError::WindowSize)
}

/// Reports `density` and the size of the Surface at `density` to `surface`, and returns the surface shown at
/// `density`.
fn apply_density(
    reports: &Reports<NativeWindow, AospStreams>,
    surface: SurfaceId,
    size: (u32, u32),
    density: Density,
) -> RuntimeSurface {
    reports.report((surface, SurfaceEvent::DensityChanged(density)));
    configure(reports, surface, size, density);
    RuntimeSurface::Shown {
        surface,
        size,
        density,
    }
}

/// Reports the size of the Surface at `density` to `surface`. A Surface of zero width or height is not configured.
fn configure(
    reports: &Reports<NativeWindow, AospStreams>,
    surface: SurfaceId,
    (width, height): (u32, u32),
    density: Density,
) {
    if width > 0 && height > 0 {
        let size = surface_size((width, height), density);
        let configured = SurfaceEvent::Configured {
            width: Some(size.width),
            height: Some(size.height),
            token: None,
        };
        reports.report((surface, configured));
    }
}

/// The size in dp of a Surface of `width` × `height` pixels at `density`.
fn surface_size((width, height): (u32, u32), density: Density) -> Size {
    let to_px = |pixels: u32| Px::new(f64::from(pixels));
    Size::from_px(to_px(width), to_px(height), density)
}
