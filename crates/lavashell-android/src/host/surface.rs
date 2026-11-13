// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use crate::native_window::NativeWindow;
use crate::platform::vulkan_window;
use crate::streams::AospStreams;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp, Px};
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
    Shown(SurfaceId),
    /// The surface closed, and is released once the renderer has retired it.
    Closing(SurfaceId),
}

impl RuntimeSurface {
    fn id(&self) -> Option<SurfaceId> {
        match *self {
            Self::Absent => None,
            Self::Requested { surface, .. } | Self::Shown(surface) | Self::Closing(surface) => {
                Some(surface)
            }
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
    /// # Errors
    /// Fails for a second surface, for a request that an AOSP host does not support, and when the size of the
    /// native window cannot be read.
    pub(crate) fn execute(
        &mut self,
        request: PlatformRequest,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let request = match request {
            PlatformRequest::Surface(request) => request,
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
                self.show(reports)
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

    /// # Errors
    /// Fails when the host reports a Surface while another one exists, and when the size of `window` cannot be read.
    pub(crate) fn created(
        &mut self,
        window: NativeWindow,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if self.window.is_some() {
            return Err(AospError::SecondWindow);
        }
        self.window = Some(window);
        self.show(reports)
    }

    /// Reports the new size of the Surface of the host to the shown surface.
    ///
    /// # Errors
    /// Fails when the size of the native window cannot be read.
    pub(crate) fn changed(
        &self,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if let (RuntimeSurface::Shown(surface), Some(window)) = (&self.surface, &self.window) {
            configure(reports, *surface, window_size(window)?);
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
            RuntimeSurface::Shown(surface) => {
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
        if let RuntimeSurface::Shown(surface) = self.surface {
            reports.time(TimingEvent::DisplayReady(surface));
        }
    }

    /// The callbacks waiting for the renderer to retire the surface.
    pub(crate) fn into_destroying(self) -> Vec<mpsc::SyncSender<()>> {
        self.destroying
    }

    /// Attaches the requested surface to the native window and reports the window size, once both exist.
    fn show(&mut self, reports: &Reports<NativeWindow, AospStreams>) -> Result<(), AospError> {
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
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        configure(reports, surface, size);
        self.surface = RuntimeSurface::Shown(surface);
        Ok(())
    }
}

fn window_size(window: &NativeWindow) -> Result<(u32, u32), AospError> {
    window.size().map_err(AospError::WindowSize)
}

/// Reports the size of the Surface of the host to `surface`. A Surface of zero width or height is not configured.
fn configure(
    reports: &Reports<NativeWindow, AospStreams>,
    surface: SurfaceId,
    (width, height): (u32, u32),
) {
    if width > 0 && height > 0 {
        let configured = SurfaceEvent::Configured {
            width: Some(at_density_one(width)),
            height: Some(at_density_one(height)),
            token: None,
        };
        reports.report((surface, configured));
    }
}

/// The host does not report a density, so one pixel of the Surface is one dp.
fn at_density_one(pixels: u32) -> Dp {
    Density::ONE.to_dp(Px::new(f64::from(pixels)))
}
