// SPDX-License-Identifier: GPL-3.0-only
use super::AospError;
use super::surface_in_window::SurfaceInWindow;
use crate::input::ShownSurface;
use crate::layout_size::LayoutSize;
use crate::native_window::NativeWindow;
use crate::output::report_entered;
use crate::platform::vulkan_window;
use crate::streams::AospStreams;
use lavashell_core::prelude::PixelSize;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::Density;
use lavashell_core::window::{Edges, WindowState};
use lavashell_platform::{
    InputRequest, PlatformRequest, Reports, Role, SurfaceAttachment, SurfaceEvent, SurfaceRequest,
    SurfaceSpec, TimingEvent,
};
use std::sync::mpsc;

#[derive(Default)]
enum RuntimeSurface {
    #[default]
    Absent,
    Requested {
        surface: SurfaceId,
        spec: Box<SurfaceSpec>,
    },
    Shown(SurfaceInWindow),
    Closing(SurfaceId),
}

impl RuntimeSurface {
    fn id(&self) -> Option<SurfaceId> {
        match self {
            Self::Absent => None,
            Self::Requested { surface, .. } | Self::Closing(surface) => Some(*surface),
            Self::Shown(shown) => Some(shown.id()),
        }
    }
}

pub(crate) struct HostSurface {
    surface: RuntimeSurface,
    window: Option<NativeWindow>,
    density: Density,
    layout_size: Option<LayoutSize>,
    destroying: Vec<mpsc::SyncSender<()>>,
}

impl HostSurface {
    pub(crate) fn new(density: Density) -> Self {
        Self {
            surface: RuntimeSurface::Absent,
            window: None,
            density,
            layout_size: None,
            destroying: Vec::new(),
        }
    }

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
        if let (RuntimeSurface::Shown(shown), Some(window)) = (&mut self.surface, &self.window) {
            shown.resize(
                window_size(window)?,
                self.density,
                self.layout_size,
                reports,
            )?;
        }
        Ok(())
    }

    pub(crate) fn change_density(
        &mut self,
        density: Density,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        if density == self.density {
            return Ok(());
        }
        self.density = density;
        if let RuntimeSurface::Shown(shown) = &mut self.surface {
            shown.change_density(density, self.layout_size, reports)?;
        }
        Ok(())
    }

    pub(crate) fn change_layout_size(
        &mut self,
        layout_size: Option<LayoutSize>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) -> Result<(), AospError> {
        self.layout_size = layout_size;
        if let RuntimeSurface::Shown(shown) = &mut self.surface {
            shown.lay_out(self.density, layout_size, reports)?;
        }
        Ok(())
    }

    pub(crate) fn destroyed(
        &mut self,
        retired: mpsc::SyncSender<()>,
        reports: &Reports<NativeWindow, AospStreams>,
    ) {
        drop(self.window.take());
        match &self.surface {
            RuntimeSurface::Shown(shown) => {
                let surface = shown.id();
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
        if let RuntimeSurface::Shown(shown) = &self.surface {
            reports.time(TimingEvent::DisplayReady(shown.id()));
        }
    }

    pub(crate) fn input_target(&self) -> Option<ShownSurface> {
        let RuntimeSurface::Shown(shown) = &self.surface else {
            return None;
        };
        shown.input_target()
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
        let size = window_size(window)?;
        let attach = spec.attach(*surface, vulkan_window(window), window.clone());
        reports.attach(SurfaceAttachment::Attach(Box::new(attach)));
        let window = display_filling_window(&spec.role);
        let shown = SurfaceInWindow::show(
            *surface,
            size,
            window,
            self.density,
            self.layout_size,
            reports,
        )?;
        report_entered(shown.id(), reports);
        self.surface = RuntimeSurface::Shown(shown);
        Ok(())
    }
}

fn window_size(window: &NativeWindow) -> Result<PixelSize, AospError> {
    window.size().map_err(AospError::WindowSize)
}

fn display_filling_window(role: &Role) -> Option<WindowState> {
    matches!(role, Role::Window(_)).then(|| WindowState {
        tiled: Edges::all(),
        ..WindowState::default()
    })
}
