// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use crate::input::ShownSurface;
use crate::native_window::NativeWindow;
use crate::platform::vulkan_window;
use crate::streams::AospStreams;
use lavashell_core::geometry::Size;
use lavashell_core::prelude::PixelSize;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::{Density, Dp, Px};
use lavashell_core::window::{Edges, WindowState};
use lavashell_platform::{
    InputRequest, PlatformRequest, Reports, Role, SurfaceAttachment, SurfaceEvent, SurfaceRequest,
    SurfaceSpec, TimingEvent,
};
use std::sync::mpsc;

const DENSITY: Density = Density::ONE;

#[derive(Default)]
enum RuntimeSurface {
    #[default]
    Absent,
    Requested {
        surface: SurfaceId,
        spec: Box<SurfaceSpec>,
    },
    Shown {
        surface: SurfaceId,
        size: PixelSize,
        window: Option<WindowState>,
    },
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

#[derive(Default)]
pub(crate) struct HostSurface {
    surface: RuntimeSurface,
    window: Option<NativeWindow>,
    destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostSurface {
    pub(crate) fn execute(
        &mut self,
        request: PlatformRequest,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        let request = match request {
            PlatformRequest::Surface(request) => request,
            PlatformRequest::Input(InputRequest::SetCursor(..)) => return Ok(()),
            unsupported => return Err(AospError::Unsupported(Box::new(unsupported))),
        };
        match request {
            SurfaceRequest::Create { surface, spec } => {
                if let Some(open) = self.surface.id() {
                    return Err(AospError::SecondSurface {
                        open,
                        requested: surface,
                    });
                }
                self.surface = RuntimeSurface::Requested { surface, spec };
                self.show(reports)
            }
            SurfaceRequest::Destroy(surface) => {
                self.require_open(surface)?;
                self.surface = RuntimeSurface::Closing(surface);
                reports.attach(SurfaceAttachment::Detach(surface));
                Ok(())
            }
            SurfaceRequest::Release(surface) => {
                self.require_open(surface)?;
                self.surface = RuntimeSurface::Absent;
                for retired in self.destroying.drain(..) {
                    let _retiring_caller_may_have_left = retired.send(());
                }
                Ok(())
            }
        }
    }

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

    pub(crate) fn changed(
        &mut self,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if let (
            RuntimeSurface::Shown {
                surface,
                size,
                window,
            },
            Some(native),
        ) = (&mut self.surface, &self.window)
        {
            *size = window_size(native)?;
            configure(reports, *surface, *size, *window);
        }
        Ok(())
    }

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
                let _retiring_caller_may_have_left = retired.send(());
            }
        }
    }

    pub(crate) fn frame(&self, reports: &Reports<NativeWindow, AospStreams>) {
        if let RuntimeSurface::Shown { surface, .. } = self.surface {
            reports.time(TimingEvent::DisplayReady(surface));
        }
    }

    pub(crate) fn shown(&self) -> Option<ShownSurface> {
        let RuntimeSurface::Shown { surface, size, .. } = self.surface else {
            return None;
        };
        Some(ShownSurface {
            id: surface,
            size: Size::new(to_dp(size.width), to_dp(size.height)),
            density: DENSITY,
        })
    }

    pub(crate) fn into_destroying(self) -> Vec<mpsc::SyncSender<()>> {
        self.destroying
    }

    fn require_open(&self, requested: SurfaceId) -> Result<(), AospError> {
        match self.surface.id() {
            Some(open) if open == requested => Ok(()),
            open => Err(AospError::UnknownSurface { requested, open }),
        }
    }

    fn show(&mut self, reports: &Reports<NativeWindow, AospStreams>) -> Result<(), AospError> {
        let (RuntimeSurface::Requested { surface, spec }, Some(window)) =
            (&self.surface, &self.window)
        else {
            return Ok(());
        };
        let surface = *surface;
        let size = window_size(window)?;
        let attach = spec.attach(surface, vulkan_window(window), window.clone());
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        let state = display_filling_window(&spec.role);
        configure(reports, surface, size, state);
        self.surface = RuntimeSurface::Shown {
            surface,
            size,
            window: state,
        };
        Ok(())
    }
}

fn window_size(window: &NativeWindow) -> Result<PixelSize, AospError> {
    window.size().map_err(AospError::WindowSize)
}

fn configure(
    reports: &Reports<NativeWindow, AospStreams>,
    surface: SurfaceId,
    size: PixelSize,
    window: Option<WindowState>,
) {
    if size.width > 0 && size.height > 0 {
        let configured = SurfaceEvent::Configured {
            width: Some(to_dp(size.width)),
            height: Some(to_dp(size.height)),
            token: None,
            window,
        };
        reports.report((surface, configured));
    }
}

fn display_filling_window(role: &Role) -> Option<WindowState> {
    matches!(role, Role::Window(_)).then(|| WindowState {
        tiled: Edges::all(),
        ..WindowState::default()
    })
}

fn to_dp(pixels: u32) -> Dp {
    DENSITY.to_dp(Px::new(f64::from(pixels)))
}
